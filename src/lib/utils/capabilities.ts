import { HARNESS_CAPABILITIES, HARNESS_INFO } from "$lib/bindings";
import type { Capability, HarnessKind } from "$lib/types";

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

/** `value` as a harness this build knows, else `null`. */
export function harnessKind(value: string | null | undefined): HarnessKind | null {
  return value && Object.hasOwn(HARNESS_INFO, value) ? (value as HarnessKind) : null;
}

/** The short name an agent goes by in prompts like "Claude wants to…". */
export function harnessName(harness: string | null | undefined): string {
  const kind = harnessKind(harness ?? "codex");
  return kind ? HARNESS_INFO[kind].name : (harness ?? "Codex");
}

/** The product name, for menus: "Claude Code", "maki". */
export function harnessLabel(harness: string | null | undefined): string {
  const kind = harnessKind(harness ?? "codex");
  return kind ? HARNESS_INFO[kind].label : (harness ?? "Codex");
}
