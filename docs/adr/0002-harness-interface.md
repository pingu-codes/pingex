---
status: accepted
---

# Drive every agent CLI through one ACP-shaped interface with native drivers

Pingex was written as a frontend for one agent CLI, Codex, and its
`app-server` protocol leaks everywhere: `HomeContext.session` is a
`CodexSession`, the frontend reducer switches on Codex method names, the
journal stores Codex items, and the app is keyed on `CODEX_HOME`. Adding
Claude Code, and later any agent that speaks the Agent Client Protocol, means
choosing what the app's internal vocabulary is. We chose an internal event and
item model shaped after ACP v1 (with tool calls as upserts and a turn-level
stop reason, as ACP v2 does), a Rust `Driver` trait that each harness
implements natively, and a Profile database that owns thread identity across
harnesses. Codex keeps its native app-server driver; the neutral thing is
Pingex's internal interface, not the wire. The cost is a translation layer per
driver and a one-time rewrite of the reducer, the journal and the item types.
The gain is that the frontend never learns a second protocol, a third harness
is one more driver, and Codex-only features survive as declared extensions
rather than as the shape everything else has to fit.

The full model is in `features/13-harnesses.md`; the vocabulary (Harness,
Home, Profile, Driver, Capability) is in `CONTEXT.md`.

## Considered options

- **Keep Codex vocabulary internally and translate Claude into it.** Every
  Claude frame would need a fake Codex method name, and the ACP driver would
  translate ACP into fake-Codex and the frontend would read fake-Codex. The
  reducer already fights Codex's own quirks (turn ids that differ between
  `turn/start` and `turn/started`, completed items that forget their deltas);
  those fixes belong in a Codex translator, not in the reducer every harness
  shares.
- **Drive everything, including Codex, through the official ACP adapters.**
  Cleanest abstraction and least code. Rejected because `codex-acp` drops the
  things that make Pingex worth using over the CLI: the queue, revert,
  projects, sections, dynamic tools, remote control, rate limits and hooks.
  `claude-agent-acp` likewise drops hooks, rewind, rename, the thinking
  budget, per-turn cost and rate-limit events. ACP stays as one generic driver
  for harnesses we have no native driver for.
- **A version-keyed capability table per harness.** Same objection as ADR
  0001: Claude Code publishes `capabilities[]` on `system/init` precisely so
  clients probe, and a source build of Codex reports `0.0.0`. Probing stays,
  extended with a declared half for what a driver knows before it spawns.
