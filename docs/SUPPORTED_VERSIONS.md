# Supported Codex versions

Pingex talks to `codex app-server` over JSON-RPC. It aims to work against three
tiers of the Codex CLI at any time:

| Tier | What it is | Version | Tag / ref | Commit | Date |
|---|---|---|---|---|---|
| **Unstable** | Upstream `main`, as mirrored in `../codex-mirror` | `0.0.0` (source builds report the workspace version) | `main` | `3a04482645b695085f4daf7c6310ab8592653fea` | 2026-09-01 |
| **Stable** | The latest tagged release | `0.152.0` | `rust-v0.152.0` | `316795b3cf2a45e90d121d9f46499d4658b2645c` | 2026-08-31 |
| **Last stable** | The release before it | `0.151.0` | `rust-v0.151.0` | `78c290807ce710180111df227df3b7a4fe845452` | 2026-08-29 |

Older releases are not tested. They mostly keep working because nothing in
the app branches on the version (see below), but a release older than *last
stable* gets a warning banner on connect, and so does anything newer than
*stable* — including an unstable build — since it is untested.

## Other harnesses

| Harness | Tested version | Protocol floor | Where the floor is enforced |
|---|---|---|---|
| Claude Code (`claude`) | `2.1.251` | `1.0.59` (`--permission-prompt-tool stdio`) | `src-tauri/src/claude/driver.rs` `status()`; the harness picker is disabled below it |

Claude Code has no tiers: the driver supports whatever is installed, probes
`system/init.capabilities[]`, and refuses to start only below the floor. The
recorded streams under `tests/fixtures/protocol/claude/` are from the tested
version. `deno task versions:check` does not cover Claude yet.

## What "support" means

- **One code path.** The app never pins a Codex version. An API that only some
  tiers have is declared as a `Feature` in `src-tauri/src/codex/compat.rs`,
  tried once, and the refusal remembered for the life of the child process
  (`CodexSession::send_gated`). The frontend gets an error prefixed with the
  feature's `error_prefix` (`src/lib/services/api.ts`, `*_UNSUPPORTED`) and
  falls back — it never sees a version number.
- **Tested against stable.** `deno task test:e2e:codex` runs the live suite
  against the installed CLI (`PINGEX_E2E_CODEX` to point it elsewhere). Each
  version-dependent test takes the modern branch where the API exists and,
  where it does not, asserts the refusal is one the classifier recognises
  *and* that the Codex really is old enough for that (`expect_legacy` in
  `src-tauri/tests/live_codex/harness.rs`, driven by `Feature::since`).
- **Unstable is best effort.** The mirror is fetched so the generated types and
  protocol reading stay ahead of the next release; unstable-only features are
  unit-tested off captured payloads and smoke-tested by hand.

## Feature × tier matrix

Gated APIs (one row per `Feature`):

| Feature | API | Last stable 0.151.0 | Stable 0.152.0 | Unstable |
|---|---|---|---|---|
| `REVERT` | `thread/revert` | ✓ | ✓ | ✓ |
| `QUEUE` | `thread/queue/*` (needs the experimental capability and a queue database) | ✓ | ✓ | ✓ |
| `PROJECTS` | `project/*` | ✓ | ✓ | ✓ |
| `SECTIONS` | `threadSection/*` | ✓ | ✓ | ✓ |
| `TURN_SETTINGS` | `turn/settings/update` — change model/effort mid-turn; needs the `step_model_switching` feature, which the app turns on with `-c` at spawn (`child::APP_SERVER_ARGS`) | ✓ | ✓ | ✓ |

Payload additions the app reads when present (no gating needed — the field is
simply absent on older tiers):

| Field / notification | Where it shows | 0.151.0 | 0.152.0 | Unstable | Live-tested |
|---|---|---|---|---|---|
| `item/commandExecution/requestApproval.kind` (`command` \| `writeStdin`) | approval card title | ✓ | ✓ | ✓ | `command` only |
| `McpServerStatus.runtimeStatus` | Integrations row | ✓ | ✓ | ✓ | ✓ |
| `TurnError.misalignment` (explanation + suggested steer) | failed-turn card | ✓ | ✓ | ✓ | unit only |
| `Project.recencyAt` + `project/list` sort | sidebar order of never-dragged projects | – | ✓ | ✓ | ✓ |
| `modelProvider/authRecovery{Started,Completed}` | header pill | – | ✓ | ✓ | unit only |
| `functionCallOutput` item | transcript work item | ✓ | ✓ | ✓ | unit only |
| `thread/{archived,unarchived,deleted,closed}`, `thread/goal/cleared`, `skills/changed`, `account/updated` | sidebar / thread view refresh | ✓ | ✓ | ✓ | ✓ |

Behaviour changes the app had to follow:

| Change | Since | What the app does |
|---|---|---|
| New threads default to paginated history, and `thread/rollback` refuses paginated threads | 0.152.0 | `/undo` sends `thread/revert` first and falls back to `thread/rollback` only on a classified "revert unsupported" refusal (`ThreadView.undoTurns`) |

Deliberately not adopted yet (tracked on the roadmap): paginated history
(`thread/turns/list`, `thread/items/list`, `thread/timeline/list` — upstream
deprecates full hydration on resume/fork in their favour), `turn/steer`,
`thread/search`, `experimentalFeature/list`, `permissionProfile/list`, the MCP
event stream, realtime/voice, plugins/marketplace, `fs/*`, process/terminal,
login/Bedrock flows, environments and the Windows sandbox, raw response events.

## Bumping a tier

When a new Codex release ships:

1. `git -C ../codex-mirror fetch --tags origin && git -C ../codex-mirror merge --ff-only origin/main`.
   (`git describe` on the mirror needs `--match 'rust-v0.*'`; it carries
   unrelated `rusty-v8-*` tags.)
2. Update the tier table above (tag, commit, date) and the same two versions
   in `src/lib/app/codexVersion.svelte.ts` (`LAST_STABLE`, `STABLE`) and the
   comment at the top of the version-dependent section of
   `src-tauri/tests/live_codex/main.rs`.
3. If a gated API became stable, set its `Feature::since` in `compat.rs` and
   move its ✓ in the matrix. If upstream added protocol the app should read,
   diff `codex-rs/app-server-protocol/src/protocol/common.rs` between the
   tags and follow the pattern in `src-tauri/src/codex/events.rs`.
4. `deno task versions:check` — fails if the table and the mirror disagree.
5. `deno task typegen`, then `deno task test:e2e:codex` against the new stable
   (and against last stable if a binary is to hand: `PINGEX_E2E_CODEX=…`).
