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
or `~/.claude` for Claude Code, maki's fixed `~/.config/maki`, on one Host. A conversation keeps its Home;
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
harness: `ClaudeDriver`, and one `AcpDriver` per ACP harness. Codex still runs
through `CodexSession`, outside the trait. Owns the child processes and
translates the wire into `HarnessEvent`s.
_Avoid_: adapter (that is what the ACP projects call theirs), session, client

**ACP spec**:
What one ACP harness needs beyond the protocol (`AcpSpec`): how to launch it,
how to check its login, its models and modes, and the habits of its wire the
translator must know. maki is the first. A new ACP harness is a `HarnessKind`
and an ACP spec.
_Avoid_: agent spec, profile, adapter

**Native session id**:
A harness's own id for a conversation when the harness picks it (ACP's
`session/new`), stored beside the Pingex thread id. Claude's session id is the
thread id, so it has none.
_Avoid_: external id, remote id

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

**Prompt part**:
One named contributor to the System prompt slice of a Context composition —
the base instructions, an AGENTS.md file, skills instructions, permissions,
environment, memory, the compaction summary, tool definitions. Exact where the
harness reports it (Claude), ≈ estimated where the app sizes the harness's own
record of the text (Codex's rollout file). The parts always add up to the slice.
_Avoid_: section (Codex's world-state word), component, category (the slice
it sits inside)

**Memory store**:
One directory of memories a harness keeps in a Home: Codex's
`$CODEX_HOME/memories`, or one Claude Code project's `projects/<slug>/memory`.
A store may have a `MEMORY.md` index that the harness loads first.
_Avoid_: memory bank, memory folder

**Memory entry**:
One file in a Memory store, shown with the title and description its
frontmatter or first line gives, and a kind (`index`, `summary`, `rollout`,
`skill`, or the type a Claude memory declares).
_Avoid_: note, memory (on its own, which means the feature)

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
fork, so the original turn's id names the whole group of Versions. Wherever
git's branches could be meant (the Review desk is full of them), say
"git branch" for those and keep "Branch" for this.
_Avoid_: fork (the operation that makes one), child thread, side question (a different hidden thread)

**Root thread**:
The thread at the top of a family of Branches — the one the sidebar lists and
highlights whichever Branch is on show. Opening it lands on the most recently
active Branch.
_Avoid_: parent (a Branch's parent may itself be a Branch), main thread

**Unadded worktree**:
A linked Git worktree of a sidebar repository that was never added to the
sidebar as a project of its own. With worktree nesting on, it shows as a muted
child row under its repository and holds the threads whose cwd is inside it;
starting a thread there does not add it.
_Avoid_: ghost project, unregistered project

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

**Review desk**:
The part of Pingex for other people's and your own pull requests: an inbox
per repository, stacks, file-by-file review with human and agent comments,
and the checkouts a PR needs. Designed in the "Pingex review desk" canvas.
_Avoid_: PR view, review panel (the older single-PR review screen)

**Inbox**:
A repository's open pull requests as of its last sync, with their review,
check and thread state, and what changed since the sync before.
_Avoid_: PR list, notifications

**Seen head**:
The head commit of a pull request that the reviewer last marked as seen.
"Changes since you last looked" is measured from it, and it stays readable
after the author force-pushes.
_Avoid_: last reviewed commit, read marker

**Stack**:
An ordered chain of pull requests, each based on the head of the one below
it, as GitHub records it. Pingex reads a Stack and never infers one from
branch names.
_Avoid_: chain, train, dependent PRs

**Checkout step**:
Where a pull request's code lives on disk, one of three, lowest first:
**No checkout** (the PR's commits fetched into `refs/pingex/pr/<n>/`, the
working tree untouched), **Review checkout**, and **Worktree** (a named
worktree on the PR's git branch, listed in the sidebar). A PR moves up only
when an action needs it, and only as far as that action needs; the table of
what each action needs is `review/requirements.rs`.
_Avoid_: checkout mode, level, stage

**Review checkout**:
A detached checkout at a pull request's head that Pingex owns and creates
only when something has to run there. It never appears in the worktree list
and is read-only to agents; making it a worktree keeps its path.
_Avoid_: temp worktree, PR worktree

**Agent review**:
A review of a pull request by an agent, run as its own thread on any
harness and filed under the PR. Its results come back as Findings.
_Avoid_: AI review, bot review, "Ask Codex" (the older composer pre-fill)

**Diff review**:
An Agent review at No checkout: the agent reads the patch, the conversation
and files at the PR head, and runs nothing.
_Avoid_: quick review, read-only review

**Finding**:
One issue an Agent review reports, at a file and line, with a severity. Only
the reviewer sees it until they add it to their Pending review; they can also
dismiss it with a reason.
_Avoid_: suggestion (GitHub's word for a proposed code change), comment

**Review rule**:
A kind of Finding the reviewer asked agents to skip in one repository
("Skip this kind of finding in <repository>"). Pingex stores it, never the
repository's files, and every later Agent review's prompt carries it.
_Avoid_: review instructions (those are the repository's own AGENTS.md or CLAUDE.md)

**Pending review**:
The reviewer's unsubmitted review of one pull request: drafted comments,
replies, suggestions and accepted Findings, a summary and a verdict, posted
to GitHub in one submission. It survives reloads and restarts.
_Avoid_: draft (alone), batch

**PR link**:
What a thread records about the pull request it is about: the repository and
PR number, plus the Checkout step, checkout path, git branch and commit it
started on. The PR decides where the thread is grouped; the checkout decides
where it runs.
_Avoid_: PR association, thread tag
