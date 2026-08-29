/**
 * Choosing a Codex home and booting against it. Codex lazily spawns the
 * app-server for whichever home is active, so nothing may boot until a home is
 * settled — hence the explicit picker phase before the app phase.
 */

import { open } from "@tauri-apps/plugin-dialog";
import { appData, refresh } from "$lib/app/appData.svelte";
import { checkCodexVersion } from "$lib/app/codexVersion.svelte";
import { closeAllDialogs } from "$lib/app/dialogs.svelte";
import { goHome } from "$lib/app/navigation.svelte";
import { refreshAccountUsage } from "$lib/services/accountUsage.svelte";
import { isTauri, readLaunchState, removeRecentHome, selectCodexHome, setCodexBinary } from "$lib/services/api";
import type { LaunchState } from "$lib/types";

export const launch = $state<{
  /** "loading" while we read launch state, "picker" to choose a Codex home
   *  before booting, "app" once a home is active and Codex has bootstrapped. */
  phase: "loading" | "picker" | "app";
  state: LaunchState | null;
  busy: boolean;
  error: string | null;
}>({
  phase: "loading",
  state: null,
  busy: false,
  error: null,
});

/** The active Codex home, preferring what bootstrap reported. */
export function codexHome(): string | null {
  return appData.data?.codexHome ?? launch.state?.codexHome ?? null;
}

/**
 * Canonical key of this window's home — the value backend events carry as
 * `codexHome`, so listeners can drop events meant for a window bound to a
 * different account. Null until launch state arrives (filter passes then).
 */
export function homeKey(): string | null {
  return launch.state?.homeKey ?? null;
}

/** True when an event tagged `codexHome` belongs to this window's home. */
export function eventMatchesHome(tag: unknown): boolean {
  if (typeof tag !== "string" || !tag) return true;
  const key = homeKey();
  return key === null || tag === key;
}

export function codexBinary(): string | null {
  return appData.data?.codexBinary ?? launch.state?.codexBinary ?? null;
}

/**
 * Bootstrap Codex (which lazily spawns the app-server against the active home)
 * and read rate limits once so the usage meter is populated before the first
 * turn. Only runs after a home is chosen so nothing spawns pre-boot.
 */
export async function boot(): Promise<void> {
  launch.phase = "app";
  await refresh();
  refreshAccountUsage();
}

export async function chooseHome(path: string): Promise<void> {
  launch.busy = true;
  launch.error = null;
  try {
    launch.state = await selectCodexHome(path);
    await boot();
  } catch (cause) {
    launch.error = cause instanceof Error ? cause.message : String(cause);
  } finally {
    launch.busy = false;
  }
}

/** Switch home for an incoming handoff; throws so the caller can report it. */
export async function switchToHome(path: string): Promise<void> {
  launch.state = await selectCodexHome(path);
  await boot();
}

// Open the native folder dialog and hand the path back to the picker, which
// shows a confirmation step instead of booting straight away.
export async function browseForHome(): Promise<string | null> {
  if (!isTauri()) return null;
  const path = await open({ directory: true, multiple: false, title: "Choose a Codex home" });
  return typeof path === "string" ? path : null;
}

// Point the app at a Codex CLI from the picker. Throws so the picker can show
// the failure inline; a success takes effect immediately (no restart).
export async function setBinary(path: string): Promise<void> {
  launch.state = await setCodexBinary(path);
  launch.error = null;
  // A different binary may be a different version.
  void checkCodexVersion();
}

// Forget a home from the picker's recents list (the folder itself is kept).
export async function removeHome(path: string): Promise<void> {
  launch.error = null;
  try {
    launch.state = await removeRecentHome(path);
  } catch (cause) {
    launch.error = cause instanceof Error ? cause.message : String(cause);
  }
}

// Return to the picker to switch homes after boot. Selecting a home reopens the
// frontend database and respawns the app-server against the new CODEX_HOME.
export async function switchHome(): Promise<void> {
  closeAllDialogs();
  goHome();
  launch.error = null;
  try {
    launch.state = await readLaunchState();
  } catch {
    // Keep the last known launch state if the refresh fails.
  }
  launch.phase = "picker";
}

// Decide whether to boot straight in (an explicit --codex-home/CODEX_HOME) or
// show the picker first so Codex does not start against the wrong home.
export async function init(): Promise<void> {
  try {
    launch.state = await readLaunchState();
    if (launch.state.needsPicker) {
      launch.phase = "picker";
      return;
    }
  } catch (cause) {
    appData.error = cause instanceof Error ? cause.message : String(cause);
  }
  await boot();
}
