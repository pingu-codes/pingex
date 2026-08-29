/**
 * Which support tier the connected Codex falls in, and the banner that says
 * so when it falls outside. The tiers are documented in
 * `docs/SUPPORTED_VERSIONS.md`; `deno task versions:check` keeps these two
 * constants in step with that table.
 *
 * Nothing else in the app branches on the version: optional APIs are probed
 * (`Feature` in `src-tauri/src/codex/compat.rs`), so an out-of-range Codex is
 * warned about, never refused.
 */
import { readCodexServerInfo } from "$lib/services/api";
import { codexVersionFromUserAgent } from "$lib/types";

/** The oldest release the app is tested against. */
export const LAST_STABLE = "0.150.1";
/** The release the app is written and tested against. */
export const STABLE = "0.151.0";

export type VersionTier = "supported" | "older" | "newer" | "unstable";

export interface VersionBanner {
  tier: VersionTier;
  version: string;
  dismissed: boolean;
}

export const codexVersion = $state<{ banner: VersionBanner | null }>({ banner: null });

/** `a` compared with `b` as dotted numbers; a pre-release suffix is ignored. */
export function compareVersions(a: string, b: string): number {
  const parse = (value: string) =>
    value
      .split("-")[0]
      .split(".")
      .map((part) => Number.parseInt(part, 10) || 0);
  const left = parse(a);
  const right = parse(b);
  for (let i = 0; i < Math.max(left.length, right.length); i++) {
    const diff = (left[i] ?? 0) - (right[i] ?? 0);
    if (diff !== 0) return diff < 0 ? -1 : 1;
  }
  return 0;
}

/**
 * Where `version` sits against the tiers. A source build reports `0.0.0`
 * (the mirror's workspace version) and is the unreleased tier, not "older".
 */
export function classifyVersion(version: string): VersionTier {
  if (version.startsWith("0.0.0")) return "unstable";
  if (compareVersions(version, LAST_STABLE) < 0) return "older";
  if (compareVersions(version, STABLE) > 0) return "newer";
  return "supported";
}

export function bannerText(banner: VersionBanner): string {
  switch (banner.tier) {
    case "older":
      return `Codex ${banner.version} is older than the last supported release (${LAST_STABLE}); some features may not work.`;
    case "newer":
      return `Codex ${banner.version} is newer than the supported release (${STABLE}); untested.`;
    case "unstable":
      return "This Codex is an unreleased source build; untested.";
    default:
      return "";
  }
}

/** Re-read the running Codex's version and raise or clear the banner. Called
 *  after every (re)connect; dismissal lasts until the next one. */
export async function checkCodexVersion(): Promise<void> {
  try {
    const info = await readCodexServerInfo();
    const version = codexVersionFromUserAgent(info.userAgent);
    if (!version) {
      codexVersion.banner = null;
      return;
    }
    const tier = classifyVersion(version);
    codexVersion.banner = tier === "supported" ? null : { tier, version, dismissed: false };
  } catch {
    // Not knowing the version is not worth a banner of its own.
    codexVersion.banner = null;
  }
}

export function dismissVersionBanner(): void {
  if (codexVersion.banner) codexVersion.banner.dismissed = true;
}
