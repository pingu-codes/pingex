# Pingex

A desktop frontend for the Codex CLI's app-server. This glossary covers the
terms Pingex uses for its relationship with the Codex versions it runs against.

## Language

**Codex**:
The upstream CLI (`codex`) whose `app-server` subcommand Pingex spawns and
talks JSON-RPC to.
_Avoid_: backend, server (ambiguous with the app-server child and the MCP servers Codex hosts)

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
and remembered as present or refused. The unit of compatibility gating.
_Avoid_: flag, capability check, version gate, feature flag
