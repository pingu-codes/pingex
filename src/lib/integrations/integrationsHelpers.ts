import type {
  McpJsonSchema,
  McpResource,
  McpResourceTemplate,
  McpServerStatus,
  McpServerSummary,
  McpTool,
} from "$lib/types";

export type IntegrationFilter = "all" | "mcp" | "skills" | "plugins" | "connections";

/** Coarse status a row's dot represents. */
export type RowStatus = "enabled" | "disabled" | "error" | "checking" | "untested";

/**
 * Resolve the status shown by a server row's dot.
 *
 * Codex's live view wins over the static config: a server `config.toml` enables
 * but that failed to start reads as an error rather than a healthy green, and a
 * server needing a sign-in is not "working" either. A server we have no status
 * for yet is `untested` — Codex may still be starting it.
 */
export function rowStatus(server: McpServerSummary, status?: McpServerStatus | null, checking = false): RowStatus {
  if (checking) return "checking";
  if (!server.enabled) return "disabled";
  if (!status) return "untested";
  if (status.error) return "error";
  if (status.authStatus === "notLoggedIn") return "error";
  // Newer Codex builds also report a lifecycle; only its unambiguous states
  // change the verdict so an unknown value keeps today's behaviour.
  if (status.runtimeStatus === "failed" || status.runtimeStatus === "authenticationRequired") return "error";
  if (status.runtimeStatus === "starting" || status.runtimeStatus === "notStarted") return "checking";
  if (status.runtimeStatus === "disabled") return "disabled";
  return "enabled";
}

const STATUS_DOT: Record<RowStatus, string> = {
  enabled: "bg-success-500",
  disabled: "bg-surface-400",
  error: "bg-error-500",
  checking: "bg-warning-500 animate-pulse",
  untested: "bg-surface-400",
};

export function statusDotClass(status: RowStatus): string {
  return STATUS_DOT[status];
}

const STATUS_LABEL: Record<RowStatus, string> = {
  enabled: "Connected",
  disabled: "Disabled",
  error: "Error",
  checking: "Checking…",
  untested: "Not started",
};

export function statusLabel(status: RowStatus): string {
  return STATUS_LABEL[status];
}

const RUNTIME_LABEL: Record<string, string> = {
  notStarted: "Not started",
  starting: "Starting…",
  running: "Running",
  ready: "Running",
  authenticationRequired: "Sign-in required",
  failed: "Failed",
  disabled: "Disabled",
};

/**
 * The lifecycle word Codex ≥0.150 reports as `runtimeStatus`, humanised. Null
 * when the build sends none or the value adds nothing to the row's dot — the
 * label is a detail beside the verdict, not a second verdict.
 */
export function runtimeStatusLabel(status?: McpServerStatus | null): string | null {
  const runtime = status?.runtimeStatus;
  if (!runtime) return null;
  return RUNTIME_LABEL[runtime] ?? runtime;
}

/** Sorted tool list for a server; `mcpServerStatus/list` keys them by name. */
export function toolsOf(status?: McpServerStatus | null): McpTool[] {
  return Object.values(status?.tools ?? {}).sort((a, b) => a.name.localeCompare(b.name));
}

export function resourcesOf(status?: McpServerStatus | null): McpResource[] {
  return status?.resources ?? [];
}

export function resourceTemplatesOf(status?: McpServerStatus | null): McpResourceTemplate[] {
  return status?.resourceTemplates ?? [];
}

/** "2 tools · 1 resource" — what the server contributes, for the details toggle. */
export function contributionSummary(status?: McpServerStatus | null): string {
  const parts: string[] = [];
  const tools = toolsOf(status).length;
  if (tools > 0) parts.push(`${tools} ${tools === 1 ? "tool" : "tools"}`);
  const resources = resourcesOf(status).length + resourceTemplatesOf(status).length;
  if (resources > 0) parts.push(`${resources} ${resources === 1 ? "resource" : "resources"}`);
  return parts.join(" · ");
}

/** Non-empty `serverInfo` fields as label/value pairs, website last. */
export function serverInfoLines(status?: McpServerStatus | null): { label: string; value: string; href?: string }[] {
  const info = status?.serverInfo;
  if (!info) return [];
  const lines: { label: string; value: string; href?: string }[] = [];
  if (info.name) lines.push({ label: "Name", value: info.name });
  if (info.version) lines.push({ label: "Version", value: info.version });
  if (info.description) lines.push({ label: "About", value: info.description });
  if (info.websiteUrl) lines.push({ label: "Website", value: info.websiteUrl, href: info.websiteUrl });
  return lines;
}

/** Mirror of the native `validate_skill_name` rule. */
export function validSkillName(name: string): boolean {
  return /^[a-z0-9][a-z0-9_-]{0,63}$/.test(name);
}

/** One-line capability/transport summary for a server row. */
export function capabilitySummary(server: McpServerSummary, status?: McpServerStatus | null): string {
  const parts: string[] = [];
  if (server.transport === "stdio") {
    parts.push(server.command ? `stdio · ${server.command}` : "stdio");
  } else if (server.transport === "http") {
    parts.push("http");
  } else {
    parts.push("unknown transport");
  }
  const version = status?.serverInfo?.version;
  if (version) parts.push(`v${version}`);
  const contribution = contributionSummary(status);
  if (contribution) parts.push(contribution);
  return parts.join(" · ");
}

/** What the auth control for a server should offer, if anything. */
export function authAction(status?: McpServerStatus | null): "signIn" | "signedIn" | "env" | null {
  if (!status) return null;
  if (status.authStatus === "notLoggedIn") return "signIn";
  if (status.authStatus === "oAuth") return "signedIn";
  if (status.authStatus === "bearerToken") return "env";
  return null;
}

/**
 * Render a tool's arguments as `name: type` lines, required ones first.
 * Only the top level — nested object schemas are shown as `object` rather than
 * expanded, which keeps a row readable without a full schema viewer.
 */
export function toolParameters(tool: McpTool): { name: string; type: string; required: boolean; hint: string }[] {
  const schema = tool.inputSchema;
  const properties = schema?.properties ?? {};
  const required = new Set(schema?.required ?? []);
  return Object.entries(properties)
    .map(([name, property]) => ({
      name,
      type: schemaType(property),
      required: required.has(name),
      hint: property.description ?? "",
    }))
    .sort((a, b) => Number(b.required) - Number(a.required) || a.name.localeCompare(b.name));
}

function schemaType(schema: McpJsonSchema): string {
  if (schema.enum?.length) return schema.enum.map((value) => JSON.stringify(value)).join(" | ");
  if (schema.type === "array") return `${schema.items ? schemaType(schema.items) : "any"}[]`;
  return schema.type ?? "any";
}

/** Whether a config dialog's env-key rows are valid (non-empty unique names). */
export function envKeysValid(keys: string[]): boolean {
  const trimmed = keys.map((key) => key.trim()).filter(Boolean);
  return trimmed.length === new Set(trimmed).size;
}

/** Parse a shell-ish args string into an argv array (whitespace split, quotes). */
export function parseArgs(input: string): string[] {
  const out: string[] = [];
  const matches = input.match(/"[^"]*"|'[^']*'|\S+/g);
  if (!matches) return out;
  for (const token of matches) {
    if ((token.startsWith('"') && token.endsWith('"')) || (token.startsWith("'") && token.endsWith("'"))) {
      out.push(token.slice(1, -1));
    } else {
      out.push(token);
    }
  }
  return out;
}

/**
 * Render an argv array back into the single-line form [`parseArgs`] accepts.
 *
 * The edit form round-trips through text, so anything with whitespace (or an
 * empty string) has to come back quoted or it would silently split on save.
 */
export function formatArgs(args: string[]): string {
  return args
    .map((arg) => {
      if (arg !== "" && !/[\s"']/.test(arg)) return arg;
      return arg.includes('"') ? `'${arg}'` : `"${arg}"`;
    })
    .join(" ");
}

/**
 * Split a SKILL.md into its YAML frontmatter lines and the markdown body, so
 * the UI can show metadata compactly instead of rendering `---` as a rule.
 */
export function splitFrontmatter(text: string): { meta: { key: string; value: string }[]; body: string } {
  const match = /^---\r?\n([\s\S]*?)\r?\n---\r?\n?/.exec(text);
  if (!match) return { meta: [], body: text };
  const meta = match[1]
    .split(/\r?\n/)
    .map((line) => /^([A-Za-z0-9_-]+):\s*(.*)$/.exec(line))
    .filter((m): m is RegExpExecArray => m !== null)
    .map((m) => ({ key: m[1], value: m[2] }));
  return { meta, body: text.slice(match[0].length) };
}
