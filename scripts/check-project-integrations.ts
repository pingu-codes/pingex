/** Offline protocol check. Uses a temporary Codex home and sends no model turns. */
import { join } from "node:path";

const root = await Deno.makeTempDir({ prefix: "pingex-integrations-" });
const home = join(root, "home");
const project = join(root, "project");
const other = join(root, "other");
const marketplace = join(project, ".agents", "plugins", "marketplace.json");
const packageRoot = join(project, "plugins", "probe");
for (const dir of [join(home, "skills", "probe"), join(project, ".codex"), join(project, ".git"), other, join(project, ".agents", "plugins"), join(packageRoot, ".codex-plugin")]) {
  await Deno.mkdir(dir, { recursive: true });
}
await Deno.writeTextFile(marketplace, JSON.stringify({ name: "local", plugins: [{ name: "probe", source: { source: "local", path: "./plugins/probe" } }] }));
await Deno.writeTextFile(join(packageRoot, ".codex-plugin", "plugin.json"), JSON.stringify({ name: "probe" }));
await Deno.writeTextFile(join(packageRoot, ".mcp.json"), JSON.stringify({ mcpServers: { echo: { command: "/usr/bin/true" } } }));
await Deno.writeTextFile(join(home, "skills", "probe", "SKILL.md"), "---\nname: probe\ndescription: Fixture for integration discovery.\n---\nFixture only.\n");
await Deno.writeTextFile(join(home, "config.toml"), `[projects.${JSON.stringify(project)}]\ntrust_level = "trusted"\n[mcp_servers.probe]\ncommand = "/usr/bin/true"\nenabled = false\n[plugins."probe@local"]\nenabled = false\n`);
await Deno.writeTextFile(join(project, ".codex", "config.toml"), '[mcp_servers.probe]\nenabled = true\n[plugins."probe@local"]\nenabled = true\n[[skills.config]]\nname = "probe"\nenabled = false\n');

const child = new Deno.Command(Deno.env.get("PINGEX_E2E_CODEX") ?? "/opt/homebrew/bin/codex", {
  args: ["app-server", "--listen", "stdio://"], cwd: home,
  env: { CODEX_HOME: home }, stdin: "piped", stdout: "piped", stderr: "null",
}).spawn();
const writer = child.stdin.getWriter();
const pending = new Map<number, { resolve: (value: any) => void; reject: (error: Error) => void }>();
let nextId = 0;
const reader = (async () => {
  let buffer = "";
  for await (const chunk of child.stdout.pipeThrough(new TextDecoderStream())) {
    buffer += chunk;
    let newline: number;
    while ((newline = buffer.indexOf("\n")) >= 0) {
      const line = buffer.slice(0, newline); buffer = buffer.slice(newline + 1);
      if (!line.trim()) continue;
      const message = JSON.parse(line);
      const call = pending.get(message.id);
      if (call) {
        pending.delete(message.id);
        if (message.error) call.reject(new Error(JSON.stringify(message.error)));
        else call.resolve(message.result);
      }
    }
  }
})();
async function call(method: string, params: unknown): Promise<any> {
  const id = ++nextId;
  const response = new Promise((resolve, reject) => pending.set(id, { resolve, reject }));
  await writer.write(new TextEncoder().encode(`${JSON.stringify({ id, method, params })}\n`));
  let timer: number | undefined;
  try {
    return await Promise.race([response, new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(`Timed out: ${method}`)), 20_000); })]);
  } finally { clearTimeout(timer); pending.delete(id); }
}
function check(value: boolean, message: string) { if (!value) throw new Error(message); }
try {
  await call("initialize", { clientInfo: { name: "pingex-integration-check", version: "1" }, capabilities: { experimentalApi: true } });
  const local = await call("config/read", { cwd: project, includeLayers: true });
  const sibling = await call("config/read", { cwd: other, includeLayers: true });
  check(local.config.mcp_servers.probe.enabled === true, "Project MCP override ignored");
  check(local.config.plugins["probe@local"].enabled === true, "Project plugin override ignored");
  check(sibling.config.mcp_servers.probe.enabled === false, "MCP override leaked into another project");
  const skills = await call("skills/list", { cwds: [project], forceReload: true });
  const skill = skills.data.flatMap((group: any) => group.skills).find((entry: any) => entry.name === "probe");
  check(!!skill, "Fixture skill was not discovered");
  console.log(`MCP and plugin project config: PASS; project isolation: PASS; project skill override applied: ${!skill.enabled}`);
  await Deno.writeTextFile(join(project, ".codex", "config.toml"), "");
  const reset = await call("config/read", { cwd: project, includeLayers: true });
  check(reset.config.mcp_servers.probe.enabled === false, "Reset did not restore inherited MCP setting");
  await call("config/batchWrite", { edits: [], reloadUserConfig: true });
  console.log("Reset to inherited config: PASS; runtime reload request: PASS");
  await call("plugin/install", { marketplacePath: marketplace, pluginName: "probe" });
  const installed = await call("plugin/installed", { cwds: [project] });
  const plugin = installed.marketplaces.flatMap((market: any) => market.plugins).find((entry: any) => entry.id === "probe@local");
  check(plugin?.installed === true, "Local installed plugin missing");
  const detail = await call("plugin/read", { pluginName: "probe", marketplacePath: marketplace, remoteMarketplaceName: null });
  check(detail.plugin.mcpServers.includes("echo"), "Plugin MCP contribution missing");
  await Deno.writeTextFile(join(project, ".codex", "config.toml"), '[plugins."probe@local"]\nenabled = false\n');
  const disabled = await call("plugin/installed", { cwds: [project] });
  const disabledPlugin = disabled.marketplaces.flatMap((market: any) => market.plugins).find((entry: any) => entry.id === "probe@local");
  check(disabledPlugin?.enabled === false, "Plugin inventory ignores project disablement");
  console.log("Installed-plugin discovery, contributions, project disablement: PASS");
  await Deno.mkdir(join(other, ".git"));
  await Deno.mkdir(join(other, ".codex"));
  await Deno.writeTextFile(join(other, ".codex", "config.toml"), '[mcp_servers.probe]\nenabled = true\n');
  const ignored = await call("config/read", { cwd: other, includeLayers: true });
  check(ignored.config.mcp_servers.probe.enabled === false, "Untrusted project override was applied");
  check(ignored.layers.some((layer: any) => layer.disabledReason), "Ignored project layer lacks an explanation");
  console.log("Untrusted project config is ignored and explained: PASS");
} finally {
  await writer.close().catch(() => {});
  try { child.kill("SIGTERM"); } catch { /* Already exited. */ }
  await child.status;
  await reader;
  await Deno.remove(root, { recursive: true });
}
