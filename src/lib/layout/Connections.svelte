<script lang="ts">
import { Check, Monitor, Pencil, Plus, QrCode, RefreshCw, Smartphone, Tablet, Unplug, X } from "@lucide/svelte";
import { openDialog } from "$lib/app/dialogs.svelte";
import TooltipButton from "$lib/components/TooltipButton.svelte";
import {
  connectionHealth,
  healthDotClass,
  healthLabel,
  lastSeenLabel,
  platformLabel,
  scopeLabel,
} from "$lib/layout/connectionState";
import RevokeConnectionDialog from "$lib/layout/RevokeConnectionDialog.svelte";
import {
  disconnectConnection,
  remotePairingStart,
  remotePairingStatus,
  renameConnection,
  revokeConnection,
} from "$lib/services/api";
import { loadConnections, reloadConnections, remoteConnections } from "$lib/services/connections.svelte";
import type { PairingInfo, RemoteConnection } from "$lib/types";

let { active = false }: { active?: boolean } = $props();

let pairingOpen = $state(false);
let pairing = $state<PairingInfo | null>(null);
let pairingClaimed = $state(false);
let pairingError = $state<string | null>(null);
let pairingTimer: ReturnType<typeof setInterval> | null = null;

let refreshing = $state(false);
let actionError = $state<string | null>(null);
let renamingId = $state<string | null>(null);
let renameValue = $state("");
let busyId = $state<string | null>(null);

const connections = $derived(remoteConnections.list);

function stopPairingTimer() {
  if (pairingTimer) {
    clearInterval(pairingTimer);
    pairingTimer = null;
  }
}

// Load whenever this section becomes visible; stop polling when hidden.
$effect(() => {
  if (active) {
    loadConnections();
  } else {
    resetPairing();
  }
  return () => stopPairingTimer();
});

function resetPairing() {
  pairingOpen = false;
  pairing = null;
  pairingClaimed = false;
  pairingError = null;
  stopPairingTimer();
}

async function connectDevice() {
  pairingOpen = true;
  pairingError = null;
  pairingClaimed = false;
  pairing = null;
  try {
    pairing = await remotePairingStart();
    stopPairingTimer();
    pairingTimer = setInterval(async () => {
      if (!pairing || pairingClaimed) return;
      try {
        const status = await remotePairingStatus(pairing.pairingCode);
        if (status.claimed) {
          pairingClaimed = true;
          stopPairingTimer();
          // A claimed device surfaces in the relay's client list; pull it in.
          await reloadConnections();
        }
      } catch {
        // Transient polling errors are fine; keep waiting.
      }
    }, 3000);
  } catch (cause) {
    pairingError = cause instanceof Error ? cause.message : String(cause);
  }
}

async function refresh() {
  refreshing = true;
  actionError = null;
  try {
    await reloadConnections();
  } catch (cause) {
    actionError = cause instanceof Error ? cause.message : String(cause);
  } finally {
    refreshing = false;
  }
}

function startRename(connection: RemoteConnection) {
  renamingId = connection.clientId;
  renameValue = connection.name;
}

function cancelRename() {
  renamingId = null;
  renameValue = "";
}

async function commitRename(connection: RemoteConnection) {
  const next = renameValue.trim();
  renamingId = null;
  if (!next || next === connection.name) return;
  actionError = null;
  try {
    await renameConnection(connection.clientId, next);
    await reloadConnections();
  } catch (cause) {
    actionError = cause instanceof Error ? cause.message : String(cause);
  }
}

async function disconnect(connection: RemoteConnection) {
  busyId = connection.clientId;
  actionError = null;
  try {
    await disconnectConnection(connection.clientId);
    await reloadConnections();
  } catch (cause) {
    actionError = cause instanceof Error ? cause.message : String(cause);
  } finally {
    busyId = null;
  }
}

async function revoke(connection: RemoteConnection) {
  if (!(await openDialog(RevokeConnectionDialog, { connection }))) return;
  busyId = connection.clientId;
  actionError = null;
  try {
    await revokeConnection(connection.clientId);
    await reloadConnections();
  } catch (cause) {
    actionError = cause instanceof Error ? cause.message : String(cause);
  } finally {
    busyId = null;
  }
}

function platformIcon(platform: string | null) {
  const value = (platform ?? "").toLowerCase();
  if (value.includes("ipad") || value.includes("tablet")) return Tablet;
  if (value.includes("mac") || value.includes("windows") || value.includes("linux") || value.includes("web"))
    return Monitor;
  return Smartphone;
}
</script>

<div>
  <div class="flex items-start justify-between gap-3">
    <div>
      <div class="text-sm font-semibold">Connections</div>
      <p class="mt-1 text-xs leading-5 text-surface-600-400">
        Devices paired to control this Codex remotely. Pairing codes stay on this Mac.
      </p>
    </div>
    <TooltipButton
      label="Refresh connections"
      type="button"
      onclick={refresh}
      aria-label="Refresh connections"
      class="btn-icon btn-icon-sm shrink-0 hover:preset-tonal text-surface-500"
    >
      <RefreshCw size={15} class={refreshing ? "animate-spin" : ""} />
    </TooltipButton>
  </div>

  <button
    type="button"
    onclick={connectDevice}
    class="btn btn-sm preset-filled-primary-500 mt-4 gap-2"
  >
    <Plus size={15} /> Connect device
  </button>

  {#if pairingOpen}
    <div class="card preset-tonal mt-3 p-3">
      {#if pairingClaimed}
        <div class="flex items-center gap-2 text-xs text-success-500">
          <Check size={14} /> Device connected.
        </div>
      {:else if pairing}
        <div class="flex items-start gap-4">
          <div class="qr shrink-0 overflow-hidden rounded-lg bg-white p-2">
            {@html pairing.qrSvg}
          </div>
          <div class="min-w-0 text-xs leading-5 text-surface-600-400">
            <p>Scan with the ChatGPT app to pair. Waiting for your device…</p>
            {#if pairing.manualPairingCode}
              <p class="mt-2">Or enter this code manually:</p>
              <p class="mt-1 break-all font-mono text-[11px] text-surface-900-100">{pairing.manualPairingCode}</p>
            {/if}
          </div>
        </div>
      {:else}
        <div class="flex items-center gap-2 text-xs text-surface-600-400">
          <QrCode size={14} /> Generating pairing code…
        </div>
      {/if}
      {#if pairingError}
        <div class="card preset-tonal-error mt-3 px-3 py-2 text-xs">{pairingError}</div>
      {/if}
    </div>
  {/if}

  {#if actionError}
    <div class="card preset-tonal-error mt-3 px-3 py-2 text-xs">{actionError}</div>
  {/if}

  <div class="mt-4 space-y-2">
    {#if connections.length === 0}
      <div class="rounded-lg border border-dashed border-surface-300-700 px-3 py-6 text-center text-xs text-surface-500">
        No devices paired yet. Use <span class="font-medium">Connect device</span> to add one.
      </div>
    {:else}
      {#each connections as connection (connection.clientId)}
        {@const health = connectionHealth(connection.lastSeen)}
        {@const Icon = platformIcon(connection.platform)}
        <div class="card border border-surface-200-800 bg-surface-50-950 p-3">
          <div class="flex items-start gap-3">
            <div class="grid size-9 shrink-0 place-items-center rounded-lg preset-tonal text-surface-600-400">
              <Icon size={18} />
            </div>
            <div class="min-w-0 flex-1">
              <div class="flex items-center gap-2">
                {#if renamingId === connection.clientId}
                  <!-- svelte-ignore a11y_autofocus -->
                  <input
                    bind:value={renameValue}
                    autofocus
                    onkeydown={(event) => {
                      if (event.key === "Enter") commitRename(connection);
                      // preventDefault so Escape only cancels the rename and
                      // doesn't bubble up to close the whole Settings panel.
                      if (event.key === "Escape") {
                        event.preventDefault();
                        cancelRename();
                      }
                    }}
                    onblur={() => commitRename(connection)}
                    class="input h-7 min-w-0 flex-1 text-sm"
                    aria-label="Device name"
                  />
                  <TooltipButton
                    label="Save name"
                    type="button"
                    onclick={() => commitRename(connection)}
                    aria-label="Save name"
                    class="btn-icon btn-icon-sm hover:preset-tonal text-success-500"
                  >
                    <Check size={14} />
                  </TooltipButton>
                  <TooltipButton
                    label="Cancel rename"
                    type="button"
                    onclick={cancelRename}
                    aria-label="Cancel rename"
                    class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500"
                  >
                    <X size={14} />
                  </TooltipButton>
                {:else}
                  <span class="truncate text-sm font-medium" title={connection.name}>{connection.name}</span>
                  <TooltipButton
                    label="Rename device"
                    type="button"
                    onclick={() => startRename(connection)}
                    aria-label="Rename device"
                    class="btn-icon btn-icon-sm shrink-0 hover:preset-tonal text-surface-500"
                  >
                    <Pencil size={12} />
                  </TooltipButton>
                {/if}
              </div>
              <div class="mt-1 flex flex-wrap items-center gap-x-2 gap-y-0.5 text-[11px] text-surface-500">
                <span class="inline-flex items-center gap-1">
                  <span class="size-1.5 rounded-full {healthDotClass(health)}"></span>
                  {healthLabel(health)}
                </span>
                <span aria-hidden="true">·</span>
                <span>{lastSeenLabel(connection.lastSeen)}</span>
                <span aria-hidden="true">·</span>
                <span>{platformLabel(connection.platform)}{connection.deviceModel ? ` (${connection.deviceModel})` : ""}</span>
                <span aria-hidden="true">·</span>
                <span>{scopeLabel(connection.scope)}</span>
              </div>
            </div>
          </div>
          <div class="mt-3 flex justify-end gap-2">
            <TooltipButton
              label="Forget this device locally without revoking its credential"
              type="button"
              onclick={() => disconnect(connection)}
              disabled={busyId === connection.clientId}
              class="btn btn-sm preset-tonal gap-1.5 disabled:opacity-40"
            >
              <Unplug size={13} /> Disconnect
            </TooltipButton>
            <button
              type="button"
              onclick={() => revoke(connection)}
              disabled={busyId === connection.clientId}
              class="btn btn-sm preset-tonal-error gap-1.5 disabled:opacity-40"
            >
              Revoke
            </button>
          </div>
        </div>
      {/each}
    {/if}
  </div>
</div>


<style>
  .qr :global(svg) {
    display: block;
    width: 140px;
    height: 140px;
  }
</style>
