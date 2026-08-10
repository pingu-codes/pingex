import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import HomePicker from "$lib/layout/HomePicker.svelte";
import type { LaunchState } from "$lib/types";

function launchState(overrides: Partial<LaunchState> = {}): LaunchState {
  return {
    codexHome: "/home/user/.codex-work",
    codexBinary: "codex",
    defaultHome: "/home/user/.codex",
    explicit: false,
    needsPicker: true,
    recentHomes: [
      { path: "/home/user/.codex-work", lastUsed: Date.now() / 1000 - 120, exists: true },
      { path: "/home/user/.codex-personal", lastUsed: Date.now() / 1000 - 90000, exists: true },
    ],
    codexBinaryStatus: {
      binary: "codex",
      resolved: "/opt/homebrew/bin/codex",
      found: true,
      message: null,
    },
    ...overrides,
  };
}

/** A launch state whose Codex CLI cannot be spawned (the Finder-PATH case). */
function missingBinary(): LaunchState {
  return launchState({
    codexBinaryStatus: {
      binary: "codex",
      resolved: null,
      found: false,
      message: "Could not find the Codex CLI (codex) on PATH.",
    },
  });
}

function setup(state = launchState(), props: Record<string, unknown> = {}) {
  const onSelect = vi.fn();
  const onBrowse = vi.fn().mockResolvedValue(null);
  const onRemove = vi.fn();
  const onSetBinary = vi.fn().mockResolvedValue(undefined);
  const result = render(HomePicker, {
    launchState: state,
    onSelect,
    onBrowse,
    onRemove,
    onSetBinary,
    ...props,
  });
  return { ...result, onSelect, onBrowse, onRemove, onSetBinary };
}

describe("HomePicker", () => {
  it("appends the default home when it is not among the recents", () => {
    setup();
    const options = screen.getAllByTestId("home-option");
    // two recents + the default home
    expect(options).toHaveLength(3);
    expect(screen.getByText("/home/user/.codex")).toBeInTheDocument();
    expect(screen.getByText("Default")).toBeInTheDocument();
  });

  it("does not duplicate a default home that is also a recent", () => {
    const state = launchState({
      defaultHome: "/home/user/.codex-work",
      recentHomes: [{ path: "/home/user/.codex-work", lastUsed: Date.now() / 1000, exists: true }],
    });
    setup(state);
    expect(screen.getAllByTestId("home-option")).toHaveLength(1);
  });

  it("selects a home by path when clicked", async () => {
    const user = userEvent.setup();
    const { onSelect } = setup();
    await user.click(screen.getByText("/home/user/.codex-personal"));
    expect(onSelect).toHaveBeenCalledWith("/home/user/.codex-personal");
  });

  it("marks missing homes as not found on disk", () => {
    const state = launchState({
      recentHomes: [{ path: "/home/user/.codex-gone", lastUsed: Date.now() / 1000, exists: false }],
    });
    setup(state);
    expect(screen.getByText("Not found on disk")).toBeInTheDocument();
  });

  it("triggers the browse callback and disables inputs while busy", async () => {
    const user = userEvent.setup();
    const { onBrowse, onSelect } = setup(launchState(), { busy: true });
    await user.click(screen.getByText("Opening…"));
    // Busy disables the option buttons so a stray click cannot select.
    await user.click(screen.getByText("/home/user/.codex-work"));
    expect(onBrowse).not.toHaveBeenCalled();
    expect(onSelect).not.toHaveBeenCalled();
  });

  it("invokes browse when not busy", async () => {
    const user = userEvent.setup();
    const { onBrowse } = setup();
    await user.click(screen.getByText("Browse for a folder"));
    expect(onBrowse).toHaveBeenCalledOnce();
  });

  it("removes a recent home without selecting it", async () => {
    const user = userEvent.setup();
    const { onRemove, onSelect } = setup();
    await user.click(screen.getByLabelText("Remove /home/user/.codex-personal from recents"));
    expect(onRemove).toHaveBeenCalledWith("/home/user/.codex-personal");
    expect(onSelect).not.toHaveBeenCalled();
  });

  it("offers no remove button on the built-in default home", () => {
    setup();
    // Two recents are removable; the synthetic default entry is not.
    expect(screen.getAllByTestId("remove-home")).toHaveLength(2);
    expect(screen.queryByLabelText("Remove /home/user/.codex from recents")).not.toBeInTheDocument();
  });

  it("shows a pending confirmation after browsing instead of selecting immediately", async () => {
    const user = userEvent.setup();
    const { onSelect, onBrowse } = setup();
    onBrowse.mockResolvedValue("/home/user/.codex-new");
    await user.click(screen.getByText("Browse for a folder"));
    expect(onSelect).not.toHaveBeenCalled();
    expect(screen.getByTestId("pending-home")).toHaveTextContent("/home/user/.codex-new");
    await user.click(screen.getByTestId("confirm-add-home"));
    expect(onSelect).toHaveBeenCalledWith("/home/user/.codex-new");
  });

  it("accepts a raw typed path and confirms it", async () => {
    const user = userEvent.setup();
    const { onSelect } = setup();
    await user.type(screen.getByTestId("raw-home-path"), "~/.codex-hidden");
    await user.click(screen.getByTestId("use-raw-path"));
    expect(onSelect).not.toHaveBeenCalled();
    expect(screen.getByTestId("pending-home")).toHaveTextContent("~/.codex-hidden");
    await user.click(screen.getByTestId("confirm-add-home"));
    expect(onSelect).toHaveBeenCalledWith("~/.codex-hidden");
  });

  it("shows the resolved Codex CLI without a form until asked to change it", async () => {
    const user = userEvent.setup();
    setup();
    expect(screen.getByTestId("binary-row")).toHaveTextContent("/opt/homebrew/bin/codex");
    expect(screen.queryByTestId("binary-path")).not.toBeInTheDocument();
    await user.click(screen.getByTestId("edit-binary"));
    expect(screen.getByTestId("binary-path")).toHaveValue("/opt/homebrew/bin/codex");
  });

  it("blocks opening or creating a home while the Codex CLI is missing", async () => {
    const user = userEvent.setup();
    const { onSelect, onBrowse } = setup(missingBinary());
    expect(screen.getByTestId("binary-missing")).toHaveTextContent("Codex CLI not found");
    await user.click(screen.getByText("/home/user/.codex-work"));
    await user.click(screen.getByText("Browse for a folder"));
    expect(onSelect).not.toHaveBeenCalled();
    expect(onBrowse).not.toHaveBeenCalled();
  });

  it("accepts a Codex binary path when the CLI is missing", async () => {
    const user = userEvent.setup();
    const { onSetBinary } = setup(missingBinary());
    await user.type(screen.getByTestId("binary-path"), "/opt/homebrew/bin/codex");
    await user.click(screen.getByTestId("save-binary"));
    expect(onSetBinary).toHaveBeenCalledWith("/opt/homebrew/bin/codex");
  });

  it("reports a rejected binary path inline and keeps the form open", async () => {
    const user = userEvent.setup();
    const onSetBinary = vi.fn().mockRejectedValue(new Error("No executable Codex CLI at /nope"));
    setup(missingBinary(), { onSetBinary });
    await user.type(screen.getByTestId("binary-path"), "/nope");
    await user.click(screen.getByTestId("save-binary"));
    expect(await screen.findByTestId("binary-error")).toHaveTextContent("No executable Codex CLI at /nope");
    expect(screen.getByTestId("binary-path")).toBeInTheDocument();
  });

  it("clears a pending path without selecting", async () => {
    const user = userEvent.setup();
    const { onSelect } = setup();
    await user.type(screen.getByTestId("raw-home-path"), "/tmp/home");
    await user.click(screen.getByTestId("use-raw-path"));
    await user.click(screen.getByLabelText("Clear selection"));
    expect(screen.queryByTestId("pending-home")).not.toBeInTheDocument();
    expect(onSelect).not.toHaveBeenCalled();
  });
});
