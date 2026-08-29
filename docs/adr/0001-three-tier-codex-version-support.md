---
status: accepted
---

# Support three Codex tiers by probing, not by version

Pingex is a frontend for a CLI that releases weekly and whose `main` moves
faster still, and users run whatever `codex` is on their PATH. We support
three tiers at once — the last stable release, the current stable release, and
upstream `main` (see `docs/SUPPORTED_VERSIONS.md`) — through a single code
path: optional APIs are declared as `Feature`s, tried once per child process,
and their refusal remembered; the app never compares version numbers except to
show a warning. This keeps the app usable on a Codex we have not tested rather
than refusing to start, at the cost of the first call to an absent API being
the one that discovers it is absent.

## Considered options

- **Pin one version.** Simplest, but a user updating `codex` (which the CLI
  nudges them to do) would break the app until we caught up.
- **Static capability table keyed by semver.** Cannot describe a source build
  (which reports `0.0.0`) or a release that has the method but withheld the
  experimental capability; the same code would still need a refusal path.
