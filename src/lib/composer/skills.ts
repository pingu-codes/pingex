/** Helpers for the composer's `$` skill picker. */

import type { SkillSummary } from "$lib/types";

/**
 * What to show for a skill. Plugin-provided skills carry a friendlier
 * `interface.displayName` ("Browser") than their namespaced protocol name
 * ("browser-use:browser"), which is what the user actually recognises.
 */
export function skillLabel(skill: SkillSummary): string {
  return skill.displayName?.trim() || skill.name;
}

/** The one-line secondary text for a skill row. */
export function skillHint(skill: SkillSummary): string {
  return skill.shortDescription?.trim() || skill.description?.trim() || skill.path;
}

/**
 * Skills matching the query, name-prefix matches first.
 *
 * Descriptions are searched too: a skill's `SKILL.md` description is what the
 * model matches on, so it is also the best thing for a person to search by —
 * `$review` should find a code-review skill whatever it happens to be called.
 */
export function filterSkills(skills: SkillSummary[], query: string): SkillSummary[] {
  const lowered = query.trim().toLowerCase();
  if (!lowered) return skills;
  const prefix: SkillSummary[] = [];
  const elsewhere: SkillSummary[] = [];
  for (const skill of skills) {
    if (skill.name.toLowerCase().startsWith(lowered)) {
      prefix.push(skill);
      continue;
    }
    const haystack = [skill.name, skill.displayName, skill.shortDescription, skill.description]
      .filter(Boolean)
      .join(" ")
      .toLowerCase();
    if (haystack.includes(lowered)) elsewhere.push(skill);
  }
  return [...prefix, ...elsewhere];
}
