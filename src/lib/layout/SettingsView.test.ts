import { render, screen, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import SettingsView from "$lib/layout/SettingsView.svelte";

function setup() {
  const onClose = vi.fn();
  render(SettingsView, {
    account: { label: "ciaran@example.com", plan: "Pro", kind: "chatgpt" },
    codexHome: "~/.codex-personal",
    codexBinary: "codex",
    onClose,
  });
  return { onClose };
}

describe("SettingsView", () => {
  it("filters the navigation as you search", async () => {
    const user = userEvent.setup();
    setup();
    // All sections present initially.
    expect(screen.getAllByTestId("settings-nav-item").length).toBeGreaterThan(5);
    await user.type(screen.getByLabelText("Search settings"), "sandbox");
    const items = screen.getAllByTestId("settings-nav-item");
    expect(items).toHaveLength(1);
    expect(items[0]).toHaveTextContent("Agent");
  });

  it("renders a config control with its value and source badge", async () => {
    const user = userEvent.setup();
    setup();
    await user.click(screen.getByRole("tab", { name: /Agent/ }));

    // Model is set in the preview config, so it shows the config.toml source.
    const model = await screen.findByRole("textbox", { name: "Model" });
    expect(model).toHaveValue("gpt-5.6-luna");

    const modelRow = model.closest("[data-testid='config-control']") as HTMLElement;
    expect(within(modelRow).getByText("config.toml")).toBeInTheDocument();

    // Approval policy is unset in the preview, so it shows the Default source.
    const approvalRow = document.querySelector(
      "[data-testid='config-control'][data-key='approval_policy']",
    ) as HTMLElement;
    expect(within(approvalRow).getByText("Default")).toBeInTheDocument();
  });

  it("toggles the message log preference from the Advanced section", async () => {
    const user = userEvent.setup();
    setup();
    await user.click(screen.getByRole("tab", { name: /Advanced/ }));

    const toggle = screen.getByRole("switch", { name: "Message log" });
    expect(toggle).toHaveAttribute("aria-checked", "false");
    // The log itself now lives in the thread's right sidebar, not inline here.
    expect(screen.queryByTestId("message-log")).not.toBeInTheDocument();

    await user.click(toggle);
    expect(toggle).toHaveAttribute("aria-checked", "true");
    expect(screen.queryByTestId("message-log")).not.toBeInTheDocument();

    await user.click(toggle);
    expect(toggle).toHaveAttribute("aria-checked", "false");
  });

  it("closes via the header button", async () => {
    const user = userEvent.setup();
    const { onClose } = setup();
    await user.click(screen.getByRole("button", { name: "Close settings" }));
    expect(onClose).toHaveBeenCalledOnce();
  });
});
