import { HARNESS_CAPABILITIES } from "$lib/bindings";
import type { Capability } from "$lib/types";

/**
 * Whether the harness behind a thread or composer can do `capability`. The
 * frontend asks this instead of branching on the harness kind (CLAUDE.md);
 * the declared set comes from the drivers (`harness::HarnessKind::capabilities`).
 * A missing harness is Codex, as on `ThreadDetail.harness`.
 */
export function can(harness: string | null | undefined, capability: Capability): boolean {
  const declared: readonly Capability[] =
    HARNESS_CAPABILITIES[(harness ?? "codex") as keyof typeof HARNESS_CAPABILITIES] ?? [];
  return declared.includes(capability);
}

const NAMES: Record<string, string> = { codex: "Codex", claude: "Claude" };

/** The short name an agent goes by in prompts like "Claude wants to…". */
export function harnessName(harness: string | null | undefined): string {
  return NAMES[harness ?? "codex"] ?? harness ?? "Codex";
}
