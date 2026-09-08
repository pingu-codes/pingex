/**
 * Where a home lives: the machine Pingex runs on, or a WSL distribution.
 * Mirrors the Rust `Host`; the backend decides everything about a host, the
 * frontend only labels it and passes it back.
 */

import type { Host } from "$lib/types";

export const NATIVE_HOST: Host = { kind: "native" };

export function wslHost(distro: string): Host {
  return { kind: "wsl", distro };
}

export function isWsl(host: Host | null | undefined): host is { kind: "wsl"; distro: string } {
  return host?.kind === "wsl";
}

/** Short badge text; empty for a native host so nothing is shown. */
export function hostLabel(host: Host | null | undefined): string {
  return isWsl(host) ? `WSL · ${host.distro}` : "";
}

/** A stable key for lists keyed by (host, path). */
export function hostKey(host: Host | null | undefined): string {
  return isWsl(host) ? `wsl:${host.distro}` : "native";
}

export function sameHost(a: Host | null | undefined, b: Host | null | undefined): boolean {
  return hostKey(a) === hostKey(b);
}

/** The `<select>` value for a host: `native` or the distribution name. */
export function hostToOption(host: Host | null | undefined): string {
  return isWsl(host) ? host.distro : "native";
}

export function hostFromOption(value: string): Host {
  return value === "native" || value === "" ? NATIVE_HOST : wslHost(value);
}

/** A non-native host, or null when native (what the settings override stores). */
export function hostOverride(host: Host | null | undefined): Host | null {
  return isWsl(host) ? host : null;
}
