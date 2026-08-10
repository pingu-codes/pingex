import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { IntegrationsList, McpServerStatus } from "$lib/types";

const listIntegrations = vi.fn();
const listMcpServerStatus = vi.fn();
const mcpOauthLogin = vi.fn();
const setMcpEnabled = vi.fn();
const setSkillEnabled = vi.fn();
const removeMcpServer = vi.fn();
const addMcpServer = vi.fn();

vi.mock("$lib/services/api", () => ({
  listIntegrations: () => listIntegrations(),
  listMcpServerStatus: () => listMcpServerStatus(),
  mcpOauthLogin: (name: string) => mcpOauthLogin(name),
  setMcpEnabled: (name: string, enabled: boolean) => setMcpEnabled(name, enabled),
  setSkillEnabled: (name: string, enabled: boolean) => setSkillEnabled(name, enabled),
  removeMcpServer: (name: string) => removeMcpServer(name),
  addMcpServer: (name: string, command: string, args: string[], env: Record<string, string>) =>
    addMcpServer(name, command, args, env),
}));

import IntegrationsSection from "$lib/integrations/IntegrationsSection.svelte";

function fixture(): IntegrationsList {
  return {
    mcpServers: [
      {
        name: "github",
        transport: "stdio",
        command: "npx",
        argCount: 2,
        url: null,
        envKeys: ["GITHUB_TOKEN"],
        bearerTokenEnvVar: null,
        enabled: true,
        scope: "global",
      },
      {
        name: "linear",
        transport: "http",
        command: null,
        argCount: 0,
        url: "https://mcp.linear.app",
        envKeys: [],
        bearerTokenEnvVar: "LINEAR_API_KEY",
        enabled: false,
        scope: "global",
      },
    ],
    skills: [
      {
        name: "code-reviewer",
        path: "~/.codex/skills/code-reviewer/SKILL.md",
        scope: "user",
        description: "Review a diff for correctness.",
        enabled: true,
        displayName: null,
        shortDescription: null,
      },
    ],
    plugins: [],
    pluginsSupported: false,
  };
}

function statuses(): Record<string, McpServerStatus> {
  return {
    github: {
      name: "github",
      serverInfo: { name: "github-mcp", title: "GitHub", version: "1.4.0" },
      tools: {
        create_issue: {
          name: "create_issue",
          description: "Open a new issue on a repository.",
          inputSchema: {
            type: "object",
            properties: {
              repo: { type: "string", description: "owner/name" },
              body: { type: "string" },
            },
            required: ["repo"],
          },
        },
      },
      authStatus: "unsupported",
    },
    linear: {
      name: "linear",
      serverInfo: null,
      tools: {},
      authStatus: "notLoggedIn",
    },
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  listIntegrations.mockResolvedValue(fixture());
  listMcpServerStatus.mockResolvedValue(statuses());
});

describe("IntegrationsSection", () => {
  it("renders MCP servers, skills, and the filter tabs", async () => {
    render(IntegrationsSection, {});
    // The live `serverInfo.title` wins over the config key when present.
    expect(await screen.findByText("GitHub")).toBeInTheDocument();
    expect(screen.getByText("linear")).toBeInTheDocument();
    expect(screen.getByText("code-reviewer")).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "MCP" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Plugins" })).toBeInTheDocument();
  });

  it("filters to only MCP rows when the MCP tab is selected", async () => {
    const user = userEvent.setup();
    render(IntegrationsSection, {});
    await screen.findByText("GitHub");
    await user.click(screen.getByRole("tab", { name: "MCP" }));
    expect(screen.queryByText("code-reviewer")).not.toBeInTheDocument();
    expect(screen.getByText("GitHub")).toBeInTheDocument();
  });

  it("toggles a server's enabled state", async () => {
    const user = userEvent.setup();
    const next = fixture();
    next.mcpServers[0].enabled = false;
    setMcpEnabled.mockResolvedValue(next);
    render(IntegrationsSection, {});
    const row = (await screen.findByText("GitHub")).closest("[id^='mcp-row-']") as HTMLElement;
    await user.click(within(row).getByRole("button", { name: "Disable" }));
    expect(setMcpEnabled).toHaveBeenCalledWith("github", false);
  });

  it("expands a server to show its tools and their parameters", async () => {
    const user = userEvent.setup();
    render(IntegrationsSection, {});
    const row = (await screen.findByText("GitHub")).closest("[id^='mcp-row-']") as HTMLElement;
    // Collapsed: the count is offered, the tool itself is not shown yet.
    expect(within(row).queryByText("create_issue")).not.toBeInTheDocument();
    await user.click(within(row).getByRole("button", { name: /1 tool/ }));
    expect(within(row).getByText("create_issue")).toBeInTheDocument();
    expect(within(row).getByText("Open a new issue on a repository.")).toBeInTheDocument();
    // Required parameters are unmarked; optional ones get a `?`.
    expect(within(row).getByText(/^repo/)).toBeInTheDocument();
    expect(within(row).getByText(/^body\?/)).toBeInTheDocument();
  });

  it("offers a sign-in for a server that needs OAuth, and not for one that does not", async () => {
    const user = userEvent.setup();
    mcpOauthLogin.mockResolvedValue(undefined);
    render(IntegrationsSection, {});
    const linear = (await screen.findByText("linear")).closest("[id^='mcp-row-']") as HTMLElement;
    const github = screen.getByText("GitHub").closest("[id^='mcp-row-']") as HTMLElement;
    expect(within(github).queryByRole("button", { name: /Sign in/ })).not.toBeInTheDocument();

    await user.click(within(linear).getByRole("button", { name: /Sign in/ }));
    expect(mcpOauthLogin).toHaveBeenCalledWith("linear");
  });

  it("toggles a skill through Codex rather than assuming it took", async () => {
    const user = userEvent.setup();
    setSkillEnabled.mockResolvedValue(undefined);
    render(IntegrationsSection, {});
    const row = (await screen.findByText("code-reviewer")).closest(".card") as HTMLElement;
    await user.click(within(row).getByRole("button", { name: "Disable" }));
    expect(setSkillEnabled).toHaveBeenCalledWith("code-reviewer", false);
    // Re-reads instead of mutating local state: once on mount, once after.
    await waitFor(() => expect(listIntegrations).toHaveBeenCalledTimes(2));
  });

  it("adds a new MCP server through the form", async () => {
    const user = userEvent.setup();
    addMcpServer.mockResolvedValue(fixture());
    render(IntegrationsSection, {});
    await screen.findByText("GitHub");
    await user.click(screen.getByRole("button", { name: /Add MCP server/ }));
    await user.type(screen.getByLabelText("Name"), "notion");
    await user.type(screen.getByLabelText("Command"), "npx");
    await user.type(screen.getByLabelText("Arguments"), "-y server-notion");
    await user.click(screen.getByRole("button", { name: "Add server" }));
    await waitFor(() => expect(addMcpServer).toHaveBeenCalledWith("notion", "npx", ["-y", "server-notion"], {}));
  });
});
