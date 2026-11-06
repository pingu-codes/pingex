import {
  Blocks,
  Bot,
  Code2,
  Database,
  FlaskConical,
  Gauge,
  Globe,
  Keyboard,
  Palette,
  Settings2,
  Sliders,
  Smartphone,
} from "@lucide/svelte";
import type { Component } from "svelte";

export type SettingsSectionId =
  | "general"
  | "appearance"
  | "agent"
  | "modelFeatures"
  | "coding"
  | "integrations"
  | "browser"
  | "connections"
  | "keyboard"
  | "usage"
  | "data"
  | "advanced";

/** Static definition of the settings navigation, kept separate from the view so
 * the search/filter logic is unit-testable without rendering. */
export interface SettingsSection {
  id: SettingsSectionId;
  label: string;
  icon: Component<{ size?: number; class?: string }>;
  /** Extra searchable terms — control names and synonyms in this section. */
  keywords: string[];
}

export const SETTINGS_SECTIONS: SettingsSection[] = [
  { id: "general", label: "General", icon: Settings2, keywords: ["codex home", "binary", "runtime", "restart"] },
  {
    id: "appearance",
    label: "Appearance",
    icon: Palette,
    keywords: [
      "theme",
      "readable",
      "high contrast",
      "accessibility",
      "font",
      "typeface",
      "density",
      "font size",
      "text size",
      "hide old threads",
      "recent threads",
      "sidebar",
    ],
  },
  {
    id: "agent",
    label: "Agent",
    icon: Bot,
    keywords: ["model", "reasoning effort", "approval policy", "sandbox mode", "subagents", "multi-agent"],
  },
  { id: "modelFeatures", label: "Model features", icon: Sliders, keywords: ["reasoning summaries", "hide reasoning"] },
  { id: "coding", label: "Coding", icon: Code2, keywords: ["file opener", "editor scheme"] },
  { id: "integrations", label: "Integrations", icon: Blocks, keywords: ["mcp servers", "skills"] },
  {
    id: "browser",
    label: "Browser",
    icon: Globe,
    keywords: ["browser use", "chrome", "tabs", "tab group", "remote debugging", "allowed sites", "cdp"],
  },
  {
    id: "connections",
    label: "Connections",
    icon: Smartphone,
    keywords: ["phone", "pairing", "qr code", "remote control"],
  },
  { id: "keyboard", label: "Keyboard shortcuts", icon: Keyboard, keywords: ["shortcuts", "hotkeys", "keybindings"] },
  {
    id: "usage",
    label: "Usage",
    icon: Gauge,
    keywords: ["tokens", "cost", "spend", "context", "cache", "system prompt"],
  },
  { id: "data", label: "Data controls", icon: Database, keywords: ["database", "drafts", "storage", "clear metadata"] },
  {
    id: "advanced",
    label: "Advanced",
    icon: FlaskConical,
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
