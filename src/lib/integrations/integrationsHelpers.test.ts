import { describe, expect, it } from "vitest";
import {
  authAction,
  capabilitySummary,
  contributionSummary,
  envKeysValid,
  formatArgs,
  parseArgs,
  rowStatus,
  runtimeStatusLabel,
  serverInfoLines,
  splitFrontmatter,
  statusDotClass,
  statusLabel,
  toolParameters,
  toolsOf,
  validSkillName,
} from "$lib/integrations/integrationsHelpers";
import type { McpServerStatus, McpServerSummary } from "$lib/types";

function server(overrides: Partial<McpServerSummary> = {}): McpServerSummary {
  return {
    name: "github",
    transport: "stdio",
    command: "npx",
    args: ["-y", "server"],
    url: null,
    envKeys: [],
    bearerTokenEnvVar: null,
    enabled: true,
    scope: "global",
    ...overrides,
  };
}

function status(overrides: Partial<McpServerStatus> = {}): McpServerStatus {
  return {
    name: "github",
    serverInfo: { name: "github-mcp", title: "GitHub", version: "1.4.0" },
    tools: {},
    authStatus: "unsupported",
    ...overrides,
  };
}

describe("rowStatus", () => {
  it("reports disabled from config regardless of live state", () => {
    expect(rowStatus(server({ enabled: false }), status())).toBe("disabled");
  });

  it("is untested until Codex has reported on the server", () => {
    // Servers start asynchronously, so "no status yet" is not "broken".
    expect(rowStatus(server({ enabled: true }), null)).toBe("untested");
  });

  it("reports connected once Codex reports the server without error", () => {
    expect(rowStatus(server({ enabled: true }), status())).toBe("enabled");
  });

  it("prefers Codex's live view over the config's enabled flag", () => {
    expect(rowStatus(server({ enabled: true }), status({ error: "exited early" }))).toBe("error");
  });

  it("treats a server awaiting sign-in as an error, not as healthy", () => {
    expect(rowStatus(server({ enabled: true }), status({ authStatus: "notLoggedIn" }))).toBe("error");
  });

  it("reports checking while a refresh runs", () => {
    expect(rowStatus(server(), status(), true)).toBe("checking");
  });

  // Codex builds after 0.149.1 add `runtimeStatus`; older ones omit it.
  it("folds the newer runtimeStatus into the verdict", () => {
    expect(rowStatus(server(), status({ runtimeStatus: "failed" }))).toBe("error");
    expect(rowStatus(server(), status({ runtimeStatus: "authenticationRequired" }))).toBe("error");
    expect(rowStatus(server(), status({ runtimeStatus: "starting" }))).toBe("checking");
    expect(rowStatus(server(), status({ runtimeStatus: "connected" }))).toBe("enabled");
  });

  it("keeps working when a newer Codex sends states it has never seen", () => {
    expect(rowStatus(server(), status({ runtimeStatus: "hibernating" }))).toBe("enabled");
    expect(rowStatus(server(), status({ authStatus: "unknown" }))).toBe("enabled");
    expect(rowStatus(server(), status({ authStatus: "somethingNew" }))).toBe("enabled");
  });
});

describe("runtimeStatusLabel", () => {
  it("humanises the 0.150 runtime status and passes unknown values through", () => {
    expect(runtimeStatusLabel(status({ runtimeStatus: "starting" }))).toBe("Starting…");
    expect(runtimeStatusLabel(status({ runtimeStatus: "authenticationRequired" }))).toBe("Sign-in required");
    expect(runtimeStatusLabel(status({ runtimeStatus: "hibernating" }))).toBe("hibernating");
    expect(runtimeStatusLabel(status({}))).toBeNull();
    expect(runtimeStatusLabel(null)).toBeNull();
  });
});

describe("statusDotClass / statusLabel", () => {
  it("maps each status to a class and label", () => {
    expect(statusDotClass("error")).toContain("error");
    expect(statusDotClass("checking")).toContain("animate-pulse");
    expect(statusLabel("disabled")).toBe("Disabled");
    expect(statusLabel("enabled")).toBe("Connected");
    expect(statusLabel("untested")).toBe("Not started");
  });
});

describe("authAction", () => {
  it("offers a sign-in only when a server has OAuth and no token", () => {
    expect(authAction(status({ authStatus: "notLoggedIn" }))).toBe("signIn");
    expect(authAction(status({ authStatus: "oAuth" }))).toBe("signedIn");
    expect(authAction(status({ authStatus: "bearerToken" }))).toBe("env");
    expect(authAction(status({ authStatus: "unsupported" }))).toBeNull();
    expect(authAction(null)).toBeNull();
  });
});

describe("toolsOf", () => {
  it("returns tools sorted by name, tolerating a server with none", () => {
    const tools = toolsOf(
      status({
        tools: {
          search_code: { name: "search_code" },
          create_issue: { name: "create_issue" },
        },
      }),
    );
    expect(tools.map((tool) => tool.name)).toEqual(["create_issue", "search_code"]);
    expect(toolsOf(null)).toEqual([]);
    expect(toolsOf(status())).toEqual([]);
  });
});

describe("toolParameters", () => {
  it("lists required parameters first and renders their types", () => {
    const parameters = toolParameters({
      name: "create_issue",
      inputSchema: {
        type: "object",
        properties: {
          body: { type: "string" },
          repo: { type: "string", description: "owner/name" },
          labels: { type: "array", items: { type: "string" } },
        },
        required: ["repo"],
      },
    });
    expect(parameters.map((parameter) => parameter.name)).toEqual(["repo", "body", "labels"]);
    expect(parameters[0]).toMatchObject({ required: true, type: "string", hint: "owner/name" });
    expect(parameters[2].type).toBe("string[]");
  });

  it("renders an enum as its alternatives", () => {
    const [parameter] = toolParameters({
      name: "x",
      inputSchema: { type: "object", properties: { mode: { enum: ["fast", "slow"] } } },
    });
    expect(parameter.type).toBe('"fast" | "slow"');
  });

  it("is empty for a tool with no schema", () => {
    expect(toolParameters({ name: "ping" })).toEqual([]);
  });
});

describe("capabilitySummary", () => {
  it("summarizes stdio transport with command", () => {
    expect(capabilitySummary(server())).toBe("stdio · npx");
  });

  it("summarizes http transport", () => {
    expect(capabilitySummary(server({ transport: "http", command: null }))).toBe("http");
  });

  it("appends the version and tool count from the live status", () => {
    expect(capabilitySummary(server(), status({ tools: { a: { name: "a" } } }))).toBe("stdio · npx · v1.4.0 · 1 tool");
    expect(capabilitySummary(server(), status({ tools: { a: { name: "a" }, b: { name: "b" } } }))).toBe(
      "stdio · npx · v1.4.0 · 2 tools",
    );
  });

  it("omits the count for a server that reported no tools", () => {
    expect(capabilitySummary(server(), status({ serverInfo: null }))).toBe("stdio · npx");
  });
});

describe("envKeysValid", () => {
  it("accepts unique non-empty names", () => {
    expect(envKeysValid(["A", "B"])).toBe(true);
    expect(envKeysValid([])).toBe(true);
  });

  it("rejects duplicates", () => {
    expect(envKeysValid(["A", "A"])).toBe(false);
  });
});

describe("parseArgs", () => {
  it("splits on whitespace", () => {
    expect(parseArgs("-y server-github")).toEqual(["-y", "server-github"]);
  });

  it("honours quotes", () => {
    expect(parseArgs('--path "/a b/c" flag')).toEqual(["--path", "/a b/c", "flag"]);
  });

  it("returns empty for blank input", () => {
    expect(parseArgs("   ")).toEqual([]);
  });
});

describe("formatArgs", () => {
  it("round-trips through parseArgs, quoting what would otherwise split", () => {
    const args = ["-y", "pkg", "--flag=a b", "", "it's"];
    expect(parseArgs(formatArgs(args))).toEqual(args);
    expect(formatArgs(["-y", "pkg"])).toBe("-y pkg");
  });
});

describe("contributionSummary", () => {
  it("counts tools and resources, omitting zeros", () => {
    expect(contributionSummary(null)).toBe("");
    expect(
      contributionSummary({
        name: "x",
        serverInfo: null,
        authStatus: "unsupported",
        tools: { a: { name: "a" } },
        resources: [{ uri: "r://1" }],
        resourceTemplates: [{ uriTemplate: "r://{id}" }],
      }),
    ).toBe("1 tool · 2 resources");
  });
});

describe("serverInfoLines", () => {
  it("skips empty fields and links the website", () => {
    expect(serverInfoLines(null)).toEqual([]);
    expect(
      serverInfoLines({
        name: "x",
        authStatus: "unsupported",
        tools: {},
        serverInfo: { name: "srv", version: null, websiteUrl: "https://e.com" },
      }),
    ).toEqual([
      { label: "Name", value: "srv" },
      { label: "Website", value: "https://e.com", href: "https://e.com" },
    ]);
  });
});

describe("validSkillName", () => {
  it("matches the native rule", () => {
    expect(validSkillName("my-skill_2")).toBe(true);
    for (const bad of ["", "My", "-x", "a b", "a/b", "a:b"]) expect(validSkillName(bad)).toBe(false);
  });
});

describe("splitFrontmatter", () => {
  it("separates yaml frontmatter from the body", () => {
    expect(splitFrontmatter("---\nname: a\ndescription: b c\n---\n\n## Hi\n")).toEqual({
      meta: [
        { key: "name", value: "a" },
        { key: "description", value: "b c" },
      ],
      body: "\n## Hi\n",
    });
    expect(splitFrontmatter("## Hi")).toEqual({ meta: [], body: "## Hi" });
  });
});
