import { fireEvent, render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import QueuedMessageRow from "$lib/thread/QueuedMessageRow.svelte";
import type { QueuedSubmission } from "$lib/types";

const skill = { type: "skill", name: "review", path: "/skills/review" } as const;
const entry: QueuedSubmission = {
  id: "q-1",
  clientUserMessageId: "c-1",
  input: [skill, { type: "text", text: "check the diff" }],
};

function setup() {
  const onEdit = vi.fn();
  render(QueuedMessageRow, { entry, canSendNow: false, onSendNow: vi.fn(), onEdit, onCancel: vi.fn() });
  return onEdit;
}

describe("QueuedMessageRow", () => {
  it("shows chips inline by name", () => {
    setup();
    expect(screen.getByText("@reviewcheck the diff")).toBeInTheDocument();
  });

  it("edits only the prose, keeping the message's chips", async () => {
    const user = userEvent.setup();
    const onEdit = setup();
    await user.click(screen.getByRole("button", { name: "Edit queued message" }));
    const field = screen.getByRole("textbox", { name: "Edit queued message" });
    expect(field).toHaveValue("check the diff");
    await user.clear(field);
    await user.type(field, "check the tests{Enter}");
    expect(onEdit).toHaveBeenCalledWith([skill, { type: "text", text: "check the tests" }]);
  });

  it("leaves an Enter that confirms an IME composition alone", async () => {
    const user = userEvent.setup();
    const onEdit = setup();
    await user.click(screen.getByRole("button", { name: "Edit queued message" }));
    const field = screen.getByRole("textbox", { name: "Edit queued message" });
    await user.type(field, " again");
    await fireEvent.keyDown(field, { key: "Enter", isComposing: true });
    expect(onEdit).not.toHaveBeenCalled();
    expect(field).toBeInTheDocument();
  });
});
