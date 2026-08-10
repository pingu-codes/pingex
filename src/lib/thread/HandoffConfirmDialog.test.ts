import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import HandoffConfirmDialog from "$lib/thread/HandoffConfirmDialog.svelte";

const props = {
  home: ".codex-work",
  threadId: "abc-123",
  dir: "/repo/wt-feature",
  command: "CODEX_HOME='/home/.codex-work' codex resume 'abc-123' --cd '/repo/wt-feature'",
  copy: vi.fn(async () => {}),
  launch: vi.fn(async () => {}),
  close: vi.fn(),
};

describe("HandoffConfirmDialog", () => {
  it("states exactly which home, thread, and directory will be used", () => {
    render(HandoffConfirmDialog, { ...props });
    // The confirmation must show the concrete home, thread id, and directory.
    expect(screen.getByText(".codex-work")).toBeInTheDocument();
    expect(screen.getByText("abc-123")).toBeInTheDocument();
    expect(screen.getByText("/repo/wt-feature")).toBeInTheDocument();
    // And the reproducible resume command.
    expect(screen.getByText(props.command)).toBeInTheDocument();
  });

  it("wires copy and launch actions, closing once the terminal opens", async () => {
    const copy = vi.fn(async () => {});
    const launch = vi.fn(async () => {});
    const close = vi.fn();
    render(HandoffConfirmDialog, { ...props, copy, launch, close });
    await userEvent.click(screen.getByRole("button", { name: /copy command/i }));
    expect(copy).toHaveBeenCalledWith(props.command);
    await userEvent.click(screen.getByRole("button", { name: /open terminal/i }));
    expect(launch).toHaveBeenCalledWith(props.command);
    expect(close).toHaveBeenCalledWith(true);
  });
});
