# Integrations: MCP, skills, plugins, and connectors

Priority: P1

## What it should do

Let users discover, install, configure, enable, disable, and test external tools. Distinguish available tools from tools enabled for the current project or thread.

## How

Add native integration commands backed by the app-server and local Codex configuration. Model connector state, capabilities, authentication state, and errors explicitly. Keep credentials native-side; expose only safe metadata to Svelte. Reuse the existing MCP activity summaries in the thread view as links back to integration details.

## What it should look like

Provide an Integrations settings section with tabs or filters for MCP, Skills, Plugins, and Connections. Each row shows icon, name, status, capabilities, scope, and a primary action such as Connect, Configure, Enable, or Repair. Include a small test-connection result inline.

## Status

Implemented, with the project skill restriction below. Additions beyond the original spec:

- **MCP details**: expanding a server shows `serverInfo` (name, version, description, website), tools with flattened parameters, and the server's resources and resource templates from `mcpServerStatus/list`.
- **Skill management**: skills can be enabled/disabled (`skills/config/write`), viewed inline (rendered `SKILL.md`), revealed in Finder / opened in Zed, created (scaffolds `~/.codex/skills/<name>/SKILL.md`, then `skills/list` with `forceReload`), and deleted — deletion is refused natively for anything outside `<codex_home>/skills/`, so only user-scope skills can go.

- Settings and Project details share a searchable Integrations view. Search matches names, descriptions, and plugin ownership and combines with the type filter.
- Project MCP and installed-plugin preferences offer Inherit, Enabled, and Disabled. Overrides live in the project's `.codex/config.toml`; resetting removes only the enabled setting. Plugin MCP contributions use their parent plugin's configuration namespace. Package installation and removal remain outside this flow.
- The list reloads on skill-change events and integration mutations. Refresh also forces skill discovery. Partial discovery failures retain previous rows and display an error.
- Codex resolves effective settings and reports ignored configuration layers. A saved override is shown separately from its effective state. Changes request runtime configuration reload without interrupting a turn; a rejected reload tells the user to start a new chat.
- Project skill enablement is unsupported by Codex 0.153.4. Its skill resolver reads user and session rules, ignoring project rules even in trusted directories. Project details therefore displays skills without a toggle; Settings still manages global skill enablement by file path. `deno run -A scripts/check-project-integrations.ts` verifies this restriction and project MCP/plugin behavior in a temporary home without model turns.
