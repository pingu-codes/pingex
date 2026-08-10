/** Static definition of the settings navigation, kept separate from the view so
 * the search/filter logic is unit-testable without rendering. */
export interface SettingsSection {
  id: string;
  label: string;
  /** Extra searchable terms — control names and synonyms in this section. */
  keywords: string[];
}

export const SETTINGS_SECTIONS: SettingsSection[] = [
  { id: "general", label: "General", keywords: ["codex home", "binary", "runtime", "restart"] },
  { id: "appearance", label: "Appearance", keywords: ["theme", "density", "font size", "text size"] },
  {
    id: "agent",
    label: "Agent",
    keywords: [
      "model",
      "reasoning effort",
      "approval policy",
      "sandbox mode",
      "subagents",
      "spawn agent",
      "parallel agents",
    ],
  },
  { id: "modelFeatures", label: "Model features", keywords: ["reasoning summaries", "hide reasoning"] },
  { id: "coding", label: "Coding", keywords: ["file opener", "editor scheme"] },
  { id: "integrations", label: "Integrations", keywords: ["mcp servers", "skills"] },
  { id: "connections", label: "Connections", keywords: ["phone", "pairing", "qr code", "remote control"] },
  { id: "keyboard", label: "Keyboard shortcuts", keywords: ["shortcuts", "hotkeys", "keybindings"] },
  { id: "data", label: "Data controls", keywords: ["database", "drafts", "storage", "clear metadata"] },
  {
    id: "advanced",
    label: "Advanced",
    keywords: ["message log", "developer", "debug", "json-rpc", "protocol", "app-server traffic"],
  },
];

/** Filter sections whose label or keywords contain the (case-insensitive)
 * query. An empty query returns every section unchanged. */
export function filterSections(sections: SettingsSection[], query: string): SettingsSection[] {
  const trimmed = query.trim().toLowerCase();
  if (!trimmed) return sections;
  return sections.filter((section) => {
    if (section.label.toLowerCase().includes(trimmed)) return true;
    return section.keywords.some((keyword) => keyword.toLowerCase().includes(trimmed));
  });
}
