# Integrations: MCP, skills, plugins, and connectors

Priority: P1

## What it should do

Let users discover, install, configure, enable, disable, and test external tools. Distinguish available tools from tools enabled for the current project or thread.

## How

Add native integration commands backed by the app-server and local Codex configuration. Model connector state, capabilities, authentication state, and errors explicitly. Keep credentials native-side; expose only safe metadata to Svelte. Reuse the existing MCP activity summaries in the thread view as links back to integration details.

## What it should look like

Provide an Integrations settings section with tabs or filters for MCP, Skills, Plugins, and Connections. Each row shows icon, name, status, capabilities, scope, and a primary action such as Connect, Configure, Enable, or Repair. Include a small test-connection result inline.
