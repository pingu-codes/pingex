---
status: accepted
---

# Agent findings come from a harness-neutral final-message JSON contract

An Agent review runs as an ordinary thread on whichever harness the reviewer
picks: Codex, Claude Code or maki. Its Findings have to come back as data the
review page can place at a file and line, give a severity and let the
reviewer add, edit or dismiss. Codex has a native `/review` with structured
output, Claude Code and maki have nothing alike, and the harness drivers must
not leak protocol vocabulary past their modules (ADR 0002). So the Findings
cannot come from any one harness's review feature.

1. **The prompt asks the agent to end its final message with one fenced
   JSON block**: a list of `{path, line, side, endLine?, severity:
   high|medium|low, title, body, evidence?, sameAsCommentId?}`, plus `pr`
   when the review covers a stack. The contract is written once, in the
   prompt the desk builds (`review/desk/agent_reviews.rs`), and read once,
   by the pure parser in `review/findings.rs`. Any harness that can write a
   message can follow it, and the parser never learns which harness wrote
   the text.

2. **The parser takes the last fenced `json` block and is strict.** A
   missing block, invalid JSON, an unknown severity or a finding without a
   path, a line or any text is an error, never a partial list. The review
   then shows "No findings parsed" with Open thread, and nothing is lost:
   the agent's reasoning is still in its thread. An empty list is a review
   that found nothing.

3. **Findings are stored per (agent review, head)** in `review_findings`,
   in one of four states: draft, added to the pending review, dismissed
   with a reason, or merged. `sameAsCommentId` merges a Finding into an
   existing human comment only when the PR has that comment; a claim
   about a comment that isn't there is dropped and the Finding stays a
   draft.

4. **What the reviewer decided feeds the next run.** Dismiss reasons from
   the previous run and the repository's review rules ("Skip this kind of
   finding in <repository>", stored per repository in Pingex, never written
   into the repository's files) go into the next agent review's prompt.

## Consequences

- Switching harness changes nothing on the review page. A new harness needs
  no findings code, only a driver.
- An agent can ignore the contract. The cost is one "No findings parsed"
  state, which the reviewer resolves by reading the thread or re-running.
- The prompt carries the contract on every run, a few hundred tokens. A
  harness with native structured output could later fill the same
  `ReportedFinding` list through a `Capability`, without changing storage
  or the UI.
