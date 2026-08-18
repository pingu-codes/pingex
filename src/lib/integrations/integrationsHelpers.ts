import type { McpJsonSchema, McpServerStatus, McpServerSummary, McpTool } from "$lib/types";

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

/** Sorted tool list for a server; `mcpServerStatus/list` keys them by name. */
export function toolsOf(status?: McpServerStatus | null): McpTool[] {
  return Object.values(status?.tools ?? {}).sort((a, b) => a.name.localeCompare(b.name));
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
  const count = toolsOf(status).length;
  if (status && count > 0) parts.push(`${count} ${count === 1 ? "tool" : "tools"}`);
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
