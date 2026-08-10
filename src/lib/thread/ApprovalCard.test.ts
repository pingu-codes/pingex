import { render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Approval } from "$lib/services/codexEvents.svelte";

const mocks = vi.hoisted(() => ({
  respondApproval: vi.fn(),
  respondServerRequest: vi.fn(),
  removeApproval: vi.fn(),
}));

vi.mock("$lib/services/api", () => ({
  respondApproval: mocks.respondApproval,
  respondServerRequest: mocks.respondServerRequest,
}));
vi.mock("$lib/services/codexEvents.svelte", () => ({ removeApproval: mocks.removeApproval }));

import ApprovalCard from "$lib/thread/ApprovalCard.svelte";

const approval: Approval = {
  requestId: 42,
  kind: "command",
  threadId: "thread-1",
  turnId: "turn-1",
  itemId: "item-1",
  command: "deno task check",
  cwd: "/tmp/project",
  reason: "Needs permission",
};

describe("ApprovalCard", () => {
  beforeEach(() => {
    mocks.respondApproval.mockReset().mockResolvedValue(undefined);
    mocks.respondServerRequest.mockReset().mockResolvedValue(undefined);
    mocks.removeApproval.mockReset();
  });

  it.each([
    ["Allow", "accept"],
    ["Allow for session", "acceptForSession"],
    ["Decline", "decline"],
  ] as const)("maps %s to %s", async (buttonName, decision) => {
    const user = userEvent.setup();
    render(ApprovalCard, { approval });

    await user.click(screen.getByRole("button", { name: new RegExp(`^${buttonName}$`) }));

    expect(mocks.respondApproval).toHaveBeenCalledWith(42, decision);
    await waitFor(() => expect(mocks.removeApproval).toHaveBeenCalledWith(42));
  });

  describe("permission requests", () => {
    const permissions: Approval = {
      requestId: 7,
      kind: "permissions",
      threadId: "thread-1",
      turnId: "turn-1",
      itemId: "item-1",
      cwd: "/tmp/project",
      permissions: { network: { enabled: true }, fileSystem: { write: ["/tmp/out"] } },
    };

    it("lists what is being asked for", () => {
      render(ApprovalCard, { approval: permissions });

      expect(screen.getByText("Codex wants extra access")).toBeInTheDocument();
      expect(screen.getByText("network access")).toBeInTheDocument();
      expect(screen.getByText("write /tmp/out")).toBeInTheDocument();
    });

    // A permission grant is answered with the profile itself, not a decision
    // word, so it goes out through respondServerRequest rather than
    // respondApproval.
    it.each([
      ["Allow", "turn"],
      ["Allow for session", "session"],
    ] as const)("grants the requested profile on %s", async (buttonName, scope) => {
      const user = userEvent.setup();
      render(ApprovalCard, { approval: permissions });

      await user.click(screen.getByRole("button", { name: new RegExp(`^${buttonName}$`) }));

      expect(mocks.respondApproval).not.toHaveBeenCalled();
      expect(mocks.respondServerRequest).toHaveBeenCalledWith(7, {
        permissions: permissions.permissions,
        scope,
      });
      await waitFor(() => expect(mocks.removeApproval).toHaveBeenCalledWith(7));
    });

    it("grants nothing on Decline", async () => {
      const user = userEvent.setup();
      render(ApprovalCard, { approval: permissions });

      await user.click(screen.getByRole("button", { name: "Decline" }));

      expect(mocks.respondServerRequest).toHaveBeenCalledWith(7, { permissions: {}, scope: "turn" });
    });
  });

  it("prevents a second decision while the first is pending", async () => {
    let resolve!: () => void;
    mocks.respondApproval.mockReturnValue(new Promise<void>((done) => (resolve = done)));
    const user = userEvent.setup();
    render(ApprovalCard, { approval });

    await user.click(screen.getByRole("button", { name: /^Allow$/ }));
    expect(screen.getByRole("button", { name: "Decline" })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "Decline" }));
    expect(mocks.respondApproval).toHaveBeenCalledOnce();

    resolve();
    await waitFor(() => expect(mocks.removeApproval).toHaveBeenCalledWith(42));
  });
});
