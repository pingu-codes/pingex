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
const saveMcpServer = vi.fn();
const readSkill = vi.fn();
const createSkill = vi.fn();
const deleteSkill = vi.fn();
const revealInFinder = vi.fn();
const openInZed = vi.fn();
const openExternalUrl = vi.fn();

vi.mock("$lib/services/api", () => ({
  listIntegrations: () => listIntegrations(),
  listMcpServerStatus: () => listMcpServerStatus(),
  mcpOauthLogin: (name: string) => mcpOauthLogin(name),
  setMcpEnabled: (name: string, enabled: boolean) => setMcpEnabled(name, enabled),
  setSkillEnabled: (name: string, enabled: boolean) => setSkillEnabled(name, enabled),
  removeMcpServer: (name: string) => removeMcpServer(name),
  saveMcpServer: (input: unknown) => saveMcpServer(input),
  readSkill: (path: string) => readSkill(path),
  createSkill: (input: unknown) => createSkill(input),
  deleteSkill: (path: string) => deleteSkill(path),
  revealInFinder: (path: string) => revealInFinder(path),
  openInZed: (path: string) => openInZed(path),
  openExternalUrl: (url: string) => openExternalUrl(url),
}));

import IntegrationsSection from "$lib/integrations/IntegrationsSection.svelte";

function fixture(): IntegrationsList {
  return {
    mcpServers: [
      {
        name: "github",
        transport: "stdio",
        command: "npx",
        args: ["-y", "@modelcontextprotocol/server-github"],
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
        args: [],
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
      {
        name: "browser-use:browser",
        path: "~/.codex/plugins/cache/browser-use/skills/browser/SKILL.md",
        scope: "system",
        description: "Drive the in-app browser.",
        enabled: true,
        displayName: "Browser",
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
      serverInfo: {
        name: "github-mcp",
        title: "GitHub",
        version: "1.4.0",
        description: "Issues and code search.",
        websiteUrl: "https://example.com/github-mcp",
      },
      resources: [{ uri: "github://repos/me/app/README.md", name: "README", mimeType: "text/markdown" }],
      resourceTemplates: [{ uriTemplate: "github://repos/{owner}/{repo}/issues/{n}", description: "One issue." }],
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
    await user.click(within(row).getByRole("button", { name: /1 tool · 2 resources/ }));
    expect(within(row).getByText("create_issue")).toBeInTheDocument();
    expect(within(row).getByText("Open a new issue on a repository.")).toBeInTheDocument();
    // Required parameters are unmarked; optional ones get a `?`.
    expect(within(row).getByText(/^repo/)).toBeInTheDocument();
    expect(within(row).getByText(/^body\?/)).toBeInTheDocument();
  });

  it("shows server info, resources, and templates when expanded", async () => {
    const user = userEvent.setup();
    render(IntegrationsSection, {});
    const row = (await screen.findByText("GitHub")).closest("[id^='mcp-row-']") as HTMLElement;
    await user.click(within(row).getByRole("button", { name: /1 tool · 2 resources/ }));
    expect(within(row).getByText("Issues and code search.")).toBeInTheDocument();
    expect(within(row).getByText("github://repos/me/app/README.md")).toBeInTheDocument();
    expect(within(row).getByText("text/markdown")).toBeInTheDocument();
    expect(within(row).getByText("github://repos/{owner}/{repo}/issues/{n}")).toBeInTheDocument();
    await user.click(within(row).getByRole("button", { name: /example\.com/ }));
    expect(openExternalUrl).toHaveBeenCalledWith("https://example.com/github-mcp");
  });

  it("loads and renders SKILL.md on demand", async () => {
    const user = userEvent.setup();
    readSkill.mockResolvedValue("---\nname: code-reviewer\n---\n\n## Instructions\n\nRead the **whole** diff.");
    render(IntegrationsSection, {});
    const row = (await screen.findByText("code-reviewer")).closest(".card") as HTMLElement;
    expect(readSkill).not.toHaveBeenCalled();
    await user.click(within(row).getByRole("button", { name: /View SKILL.md/ }));
    expect(readSkill).toHaveBeenCalledWith("~/.codex/skills/code-reviewer/SKILL.md");
    expect(await within(row).findByText("Instructions")).toBeInTheDocument();
    expect(within(row).getByText("whole")).toBeInTheDocument();
  });

  it("reveals and opens a skill's files", async () => {
    const user = userEvent.setup();
    render(IntegrationsSection, {});
    const row = (await screen.findByText("code-reviewer")).closest(".card") as HTMLElement;
    await user.click(within(row).getByRole("button", { name: "Reveal code-reviewer in Finder" }));
    expect(revealInFinder).toHaveBeenCalledWith("~/.codex/skills/code-reviewer/SKILL.md");
    await user.click(within(row).getByRole("button", { name: "Open code-reviewer in Zed" }));
    expect(openInZed).toHaveBeenCalledWith("~/.codex/skills/code-reviewer/SKILL.md");
  });

  it("deletes user skills after confirmation, and never offers to delete system ones", async () => {
    const user = userEvent.setup();
    const next = fixture();
    next.skills = next.skills.filter((skill) => skill.name !== "code-reviewer");
    deleteSkill.mockResolvedValue(next);
    render(IntegrationsSection, {});
    const system = (await screen.findByText("Browser")).closest(".card") as HTMLElement;
    expect(within(system).queryByRole("button", { name: /Delete/ })).not.toBeInTheDocument();
    const row = screen.getByText("code-reviewer").closest(".card") as HTMLElement;
    await user.click(within(row).getByRole("button", { name: "Delete code-reviewer" }));
    expect(deleteSkill).not.toHaveBeenCalled();
    await user.click(within(row).getByRole("button", { name: "Confirm delete" }));
    expect(deleteSkill).toHaveBeenCalledWith("~/.codex/skills/code-reviewer/SKILL.md");
    await waitFor(() => expect(screen.queryByText("code-reviewer")).not.toBeInTheDocument());
  });

  it("creates a skill through the form and validates the name", async () => {
    const user = userEvent.setup();
    const next = fixture();
    next.skills.push({
      name: "release-notes",
      path: "~/.codex/skills/release-notes/SKILL.md",
      scope: "user",
      description: "Draft release notes.",
      enabled: true,
      displayName: null,
      shortDescription: null,
    });
    createSkill.mockResolvedValue(next);
    render(IntegrationsSection, {});
    await screen.findByText("code-reviewer");
    await user.click(screen.getByRole("button", { name: /Add skill/ }));
    await user.type(screen.getByLabelText("Name"), "Release Notes");
    await user.type(screen.getByLabelText("Description"), "Draft release notes.");
    await user.click(screen.getByRole("button", { name: "Create skill" }));
    expect(createSkill).not.toHaveBeenCalled();
    expect(screen.getByText(/Name must be lowercase/)).toBeInTheDocument();
    await user.clear(screen.getByLabelText("Name"));
    await user.type(screen.getByLabelText("Name"), "release-notes");
    await user.click(screen.getByRole("button", { name: "Create skill" }));
    expect(createSkill).toHaveBeenCalledWith({
      name: "release-notes",
      description: "Draft release notes.",
      body: null,
    });
    expect(await screen.findByText("release-notes")).toBeInTheDocument();
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
    saveMcpServer.mockResolvedValue(fixture());
    render(IntegrationsSection, {});
    await screen.findByText("GitHub");
    await user.click(screen.getByRole("button", { name: /Add MCP server/ }));
    await user.type(screen.getByLabelText("Name"), "notion");
    await user.type(screen.getByLabelText("Command"), "npx");
    await user.type(screen.getByLabelText("Arguments"), "-y server-notion");
    await user.click(screen.getByRole("button", { name: "Add server" }));
    await waitFor(() =>
      expect(saveMcpServer).toHaveBeenCalledWith({
        previousName: null,
        name: "notion",
        command: "npx",
        args: ["-y", "server-notion"],
        env: {},
        envKeys: [],
      }),
    );
  });

  it("prefills an existing stdio server and saves edits, including a rename", async () => {
    const user = userEvent.setup();
    saveMcpServer.mockResolvedValue(fixture());
    render(IntegrationsSection, {});
    const row = (await screen.findByText("GitHub")).closest(".card") as HTMLElement;
    await user.click(within(row).getByRole("button", { name: "Edit" }));

    // Args round-trip: editing must not silently drop them.
    expect(screen.getByLabelText<HTMLInputElement>("Arguments").value).toBe("-y @modelcontextprotocol/server-github");
    expect(screen.getByLabelText<HTMLInputElement>("Command").value).toBe("npx");

    const name = screen.getByLabelText("Name");
    await user.clear(name);
    await user.type(name, "gh");
    await user.click(screen.getByRole("button", { name: "Save changes" }));
    await waitFor(() =>
      expect(saveMcpServer).toHaveBeenCalledWith({
        previousName: "github",
        name: "gh",
        command: "npx",
        args: ["-y", "@modelcontextprotocol/server-github"],
        // No value typed, so the stored secret is preserved by omission while
        // the key stays in the desired set.
        env: {},
        envKeys: ["GITHUB_TOKEN"],
      }),
    );
  });

  it("edits an HTTP server's url and bearer token variable", async () => {
    const user = userEvent.setup();
    saveMcpServer.mockResolvedValue(fixture());
    render(IntegrationsSection, {});
    const row = (await screen.findByText("linear")).closest(".card") as HTMLElement;
    await user.click(within(row).getByRole("button", { name: "Edit" }));

    expect(screen.getByLabelText<HTMLInputElement>("URL").value).toBe("https://mcp.linear.app");
    const url = screen.getByLabelText("URL");
    await user.clear(url);
    await user.type(url, "https://mcp.linear.app/sse");
    await user.click(screen.getByRole("button", { name: "Save changes" }));
    await waitFor(() =>
      expect(saveMcpServer).toHaveBeenCalledWith({
        previousName: "linear",
        name: "linear",
        url: "https://mcp.linear.app/sse",
        bearerTokenEnvVar: "LINEAR_API_KEY",
      }),
    );
  });
});
