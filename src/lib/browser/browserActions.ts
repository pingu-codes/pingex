// One line per `pingex_browser` tool call, for the thread transcript.

export const BROWSER_NAMESPACE = "pingex_browser";

function str(value: unknown): string {
  return typeof value === "string" ? value : "";
}

function shortUrl(url: string): string {
  try {
    const parsed = new URL(url);
    if (parsed.protocol === "file:") return parsed.pathname.split("/").pop() || url;
    if (parsed.protocol === "data:") return "a data: page";
    const path = parsed.pathname === "/" ? "" : parsed.pathname;
    return `${parsed.host.replace(/^www\./, "")}${path}`;
  } catch {
    return url;
  }
}

function quoted(text: string, limit = 40): string {
  const clipped = text.length > limit ? `${text.slice(0, limit - 1)}…` : text;
  return `“${clipped}”`;
}

/** What the agent did, in a few words: "Open github.com/x", "Click e12 in t1". */
export function describeBrowserCall(tool: string | undefined, args: Record<string, unknown> | undefined): string {
  const a = args ?? {};
  const tab = str(a.tab);
  const on = tab ? ` in ${tab}` : "";
  const ref = str(a.ref);
  switch (tool) {
    case "tabs":
      return "List tabs";
    case "open":
      return `Open ${shortUrl(str(a.url))}`;
    case "navigate": {
      const url = str(a.url);
      if (["back", "forward", "reload"].includes(url.toLowerCase())) {
        return `${url[0].toUpperCase()}${url.slice(1).toLowerCase()}${on}`;
      }
      return `Go to ${shortUrl(url)}${on}`;
    }
    case "snapshot":
      return `Read ${ref ? ref : "the page"}${on}`;
    case "click":
      return `${a.double ? "Double-click" : "Click"} ${ref}${on}`;
    case "type":
      return `Type ${quoted(str(a.text))} into ${ref}${on}`;
    case "press":
      return `Press ${str(a.key)}${on}`;
    case "select":
      return `Choose ${quoted(str(a.value))} in ${ref}${on}`;
    case "scroll":
      return ref ? `Scroll to ${ref}${on}` : `Scroll${on}`;
    case "wait_for":
      return `Wait for ${a.text ? quoted(str(a.text)) : a.url ? `URL ${quoted(str(a.url))}` : ref}${on}`;
    case "read":
      return `Read text${ref ? ` of ${ref}` : ""}${on}`;
    case "evaluate":
      return `Run JavaScript${on}`;
    case "screenshot":
      return `Screenshot${ref ? ` of ${ref}` : a.fullPage ? " of the full page" : ""}${on}`;
    case "take":
      return a.match ? `Take over the tab matching ${quoted(str(a.match))}` : "Take over the active tab";
    case "release":
      return `Hand back ${tab}`;
    case "close":
      return `Close ${tab}`;
    case "console":
      return `Console${on}`;
    case "network":
      return `Network log${on}`;
    case "cdp":
      return `CDP ${str(a.method)}${tab && tab !== "browser" ? on : tab === "browser" ? " on the browser" : ""}`;
    case "cdp_events":
      return `CDP events${on}`;
    default:
      return tool ?? "Browser";
  }
}
